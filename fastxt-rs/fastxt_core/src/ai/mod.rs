/*
    Fastxt
    Copyright (C) 2020  Yi Wang

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU Affero General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU Affero General Public License for more details.

    You should have received a copy of the GNU Affero General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

//! On-device AI: backend transports, prompts, and batch jobs.
//!
//! [`AiBackend`] implementations are dumb transports (Ollama, any
//! OpenAI-compatible server such as llama.cpp or Foundry Local, a mock).
//! Prompts, structured-output schemas, self-consistency voting and model
//! selection live once, in [`Ai`], so every backend behaves the same.
//!
//! Without the `ai` cargo feature the HTTP backends are absent and
//! [`Ai::check`] reports unavailable — everything else still compiles.

use crate::error::{Error, Result};
use crate::model::{AiSettings, Note};
use crate::store::Fastxt;
use serde_json::Value;

pub mod mock;
#[cfg(feature = "ai")]
pub mod ollama;
#[cfg(feature = "ai")]
pub mod openai_compatible;

/// Default categories offered when the note collection has none yet.
pub const DEFAULT_CATEGORIES: &[&str] = &["work", "personal", "reference", "idea", "task", "other"];

/// Result of probing a backend: reachable or not, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCheck {
    pub ok: bool,
    pub message: String,
}

impl AiCheck {
    fn ok(message: impl Into<String>) -> Self {
        AiCheck {
            ok: true,
            message: message.into(),
        }
    }
    fn fail(message: impl Into<String>) -> Self {
        AiCheck {
            ok: false,
            message: message.into(),
        }
    }
}

/// One text-generation request. `json_schema` asks for structured output.
pub struct GenerateRequest<'a> {
    pub prompt: &'a str,
    pub system: Option<&'a str>,
    pub json_schema: Option<&'a Value>,
    pub model: &'a str,
    pub max_tokens: u32,
}

/// A text-generation + embedding transport. Implementations never panic.
pub trait AiBackend: Send + Sync {
    /// Backend identifier as used in [`AiSettings::backend`].
    fn name(&self) -> &'static str;
    /// Probe the configured endpoint (and, when cheap, the models).
    fn check(&self, settings: &AiSettings) -> AiCheck;
    /// Generate one completion.
    fn generate(
        &self,
        request: &GenerateRequest<'_>,
        settings: &AiSettings,
    ) -> std::result::Result<String, String>;
    /// Embed one text.
    fn embed(
        &self,
        text: &str,
        model: &str,
        settings: &AiSettings,
    ) -> std::result::Result<Vec<f32>, String>;
}

/// The configured AI: prompts, voting and model selection over a backend.
pub struct Ai {
    backend: Box<dyn AiBackend>,
    settings: AiSettings,
}

impl Ai {
    /// Build from stored settings, choosing the backend by name.
    #[must_use]
    pub fn from_settings(settings: AiSettings) -> Self {
        Ai {
            backend: make_backend(&settings),
            settings,
        }
    }

    /// Build from the database's stored settings.
    ///
    /// # Errors
    /// Fails if the settings cannot be read.
    pub fn for_db(db: &Fastxt) -> Result<Self> {
        Ok(Self::from_settings(db.settings()?.ai))
    }

    /// Build with an explicit backend (tests).
    #[must_use]
    pub fn with_backend(backend: Box<dyn AiBackend>, settings: AiSettings) -> Self {
        Ai { backend, settings }
    }

    #[must_use]
    pub fn settings(&self) -> &AiSettings {
        &self.settings
    }

    /// Probe the configured endpoint — not localhost — and describe what
    /// was found. `ok` is true only if the endpoint answers.
    #[must_use]
    pub fn check(&self) -> AiCheck {
        self.backend.check(&self.settings)
    }

    /// Convenience: [`Ai::check`] without the message.
    #[must_use]
    pub fn is_available(&self) -> bool {
        self.check().ok
    }

    fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        schema: Option<&Value>,
        model: &str,
        max_tokens: u32,
    ) -> Result<String> {
        self.backend
            .generate(
                &GenerateRequest {
                    prompt,
                    system,
                    json_schema: schema,
                    model,
                    max_tokens,
                },
                &self.settings,
            )
            .map_err(|e| Error::Ai(format!("{}: {e}", self.backend.name())))
    }

    /// Suggest tags for `text`, preferring `vocabulary` tags when they fit.
    /// With `consistency_rounds > 1` the model votes and only majority tags
    /// survive. Tags are lowercase words or short phrases.
    ///
    /// # Errors
    /// [`Error::AiUnavailable`] when the backend is down; [`Error::Ai`] when
    /// the response can't be parsed.
    pub fn suggest_tags(&self, text: &str, vocabulary: &[String]) -> Result<Vec<String>> {
        let text = truncate(text, 8000);
        let vocab_line = if vocabulary.is_empty() {
            String::new()
        } else {
            format!(
                "\nPrefer reusing these existing tags when they fit: {}\n",
                vocabulary
                    .iter()
                    .take(40)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let prompt = format!(
            "Suggest 3 to 7 short lowercase tags (1-3 words each) for the note below.\n\
             {vocab_line}\nNote:\n{text}"
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "tags": { "type": "array", "items": { "type": "string" } } },
            "required": ["tags"]
        });
        let rounds = self.settings.consistency_rounds.max(1) as usize;
        let model = self.settings.model.as_str();
        let mut rounds_tags = Vec::with_capacity(rounds);
        for _ in 0..rounds {
            let response = self.generate(
                &prompt,
                Some("You tag notes. Reply with JSON only."),
                Some(&schema),
                model,
                128,
            )?;
            let tags = parse_tags(&response)?;
            rounds_tags.push(tags);
        }
        Ok(majority_vote(rounds_tags))
    }

    /// Summarize `text` in one or two sentences.
    ///
    /// # Errors
    /// See [`Ai::suggest_tags`].
    pub fn summarize(&self, text: &str) -> Result<String> {
        let text = truncate(text, 12_000);
        let response = self.generate(
            &format!("Summarize this note in 1-2 plain sentences:\n\n{text}"),
            Some("You summarize notes. Reply with the summary only, no preamble."),
            None,
            &self.settings.model,
            256,
        )?;
        if response.trim().is_empty() {
            return Err(Error::Ai("the model returned an empty summary".into()));
        }
        Ok(response)
    }

    /// Assign each text one category from `allowed` (same length out as in).
    ///
    /// # Errors
    /// See [`Ai::suggest_tags`]; also fails when the model returns the wrong
    /// number of categories.
    pub fn categorize(&self, texts: &[&str], allowed: &[String]) -> Result<Vec<String>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        if allowed.is_empty() {
            return Err(Error::Invalid("no categories to choose from".into()));
        }
        let list = texts
            .iter()
            .enumerate()
            .map(|(i, t)| format!("{}. {}\n", i + 1, truncate(t, 300)))
            .collect::<String>();
        let prompt = format!(
            "Assign each note below to exactly one of these categories:\n{}\n\n\
             Return one category per note, in order, as JSON.\n\nNotes:\n{list}",
            allowed.join(", ")
        );
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "categories": { "type": "array", "items": { "type": "string" } } },
            "required": ["categories"]
        });
        let response = self.generate(
            &prompt,
            Some("You sort notes into categories. Reply with JSON only."),
            Some(&schema),
            &self.settings.model,
            512,
        )?;
        let parsed = parse_string_array(&response, "categories")?;
        if parsed.len() != texts.len() {
            return Err(Error::Ai(format!(
                "the model returned {} categories for {} notes",
                parsed.len(),
                texts.len()
            )));
        }
        // Snap model output onto the allowed vocabulary.
        let lower: Vec<String> = allowed.iter().map(|c| c.trim().to_lowercase()).collect();
        Ok(parsed
            .into_iter()
            .map(|c| {
                let c = c.trim().to_lowercase();
                lower
                    .iter()
                    .find(|a| **a == c)
                    .or_else(|| {
                        lower
                            .iter()
                            .find(|a| a.starts_with(&c) || c.starts_with(a.as_str()))
                    })
                    .cloned()
                    .unwrap_or_else(|| "other".to_string())
            })
            .collect())
    }

    /// Embed `text` with the configured embedding model (kept separate from
    /// the chat model because chat models make poor embedders).
    ///
    /// # Errors
    /// See [`Ai::suggest_tags`].
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.backend
            .embed(
                truncate(text, 8000),
                &self.settings.embedding_model,
                &self.settings,
            )
            .map_err(|e| Error::Ai(format!("{}: {e}", self.backend.name())))
    }

    /// The model id embeddings are stored under.
    #[must_use]
    pub fn embedding_model_id(&self) -> &str {
        &self.settings.embedding_model
    }
}

fn make_backend(settings: &AiSettings) -> Box<dyn AiBackend> {
    #[cfg(feature = "ai")]
    match settings.backend.as_str() {
        "llamacpp" | "foundry-local" | "openai" => Box::new(
            openai_compatible::OpenAiCompatibleBackend::new(&settings.backend),
        ),
        _ => Box::new(ollama::OllamaBackend),
    }
    #[cfg(not(feature = "ai"))]
    {
        let _ = settings;
        Box::new(mock::UnavailableBackend)
    }
}

/// The vocabulary `categorize` uses: existing categories first, defaults when
/// the collection has none, always deduplicated.
#[must_use]
pub fn category_vocabulary(existing: &[String]) -> Vec<String> {
    let mut out: Vec<String> = existing
        .iter()
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    for d in DEFAULT_CATEGORIES {
        let d = (*d).to_string();
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Parsing helpers
// ---------------------------------------------------------------------------

/// Cut `text` to at most `max_bytes` on a UTF-8 character boundary.
#[must_use]
pub(crate) fn truncate(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Extract the first balanced JSON value embedded in a response.
#[must_use]
pub(crate) fn extract_json(response: &str) -> Option<&str> {
    let s = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```");
    let s = s.trim_end_matches("```").trim();
    let start = s.find(['{', '['])?;
    let open = s.as_bytes()[start];
    let close = if open == b'{' { '}' } else { ']' };
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s[start..].char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            c if c as u32 == open as u32 => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_tags(response: &str) -> Result<Vec<String>> {
    let tags = parse_string_array(response, "tags")?;
    let mut cleaned: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().trim_matches('#').to_lowercase();
        let words: Vec<&str> = tag.split_whitespace().collect();
        let tag = words.join(" ");
        if !tag.is_empty() && tag.chars().count() <= 40 && !cleaned.contains(&tag) {
            cleaned.push(tag);
        }
    }
    Ok(cleaned)
}

/// Parse `{"<field>": ["a", "b"]}` (or a bare array) from a model response.
fn parse_string_array(response: &str, field: &str) -> Result<Vec<String>> {
    let json = extract_json(response).ok_or_else(|| {
        Error::Ai(format!(
            "the model response is not JSON: {}",
            truncate(response.trim(), 120)
        ))
    })?;
    let value: Value = serde_json::from_str(json)?;
    match &value {
        Value::Object(map) => match map.get(field).and_then(Value::as_array) {
            Some(items) => Ok(items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()),
            None => Err(Error::Ai(format!("JSON is missing the \"{field}\" field"))),
        },
        Value::Array(items) => Ok(items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()),
        _ => Err(Error::Ai("unexpected JSON shape".into())),
    }
}

/// Keep tags that win a majority across rounds, ordered by first appearance.
fn majority_vote(rounds: Vec<Vec<String>>) -> Vec<String> {
    if rounds.len() <= 1 {
        return rounds.into_iter().next().unwrap_or_default();
    }
    let threshold = rounds.len().div_ceil(2);
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for tags in &rounds {
        let mut seen = std::collections::HashSet::new();
        for tag in tags {
            if seen.insert(tag.to_lowercase()) {
                *counts.entry(tag.to_lowercase()).or_insert(0) += 1;
            }
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for tags in &rounds {
        for tag in tags {
            let key = tag.to_lowercase();
            if counts.get(&key).copied().unwrap_or(0) >= threshold && taken.insert(key) {
                out.push(tag.clone());
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Batch jobs
// ---------------------------------------------------------------------------

/// Outcome of a batch AI job.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobReport {
    pub processed: u32,
    pub errors: u32,
}

/// AI-tag up to `limit` notes that have no AI tags yet. `progress` receives
/// (done, total).
///
/// # Errors
/// [`Error::AiUnavailable`] when the backend is unreachable.
pub fn tag_notes(db: &mut Fastxt, limit: u32, progress: &dyn Fn(u32, u32)) -> Result<JobReport> {
    let ai = Ai::for_db(db)?;
    let check = ai.check();
    if !check.ok {
        return Err(Error::AiUnavailable(check.message));
    }
    let vocabulary: Vec<String> = db.tag_vocabulary(40)?.into_iter().map(|t| t.tag).collect();
    let notes = db.notes_without_ai_tags(limit)?;
    let total = notes.len() as u32;
    let mut report = JobReport::default();
    for (done, note) in notes.iter().enumerate() {
        progress(done as u32, total);
        match ai.suggest_tags(&note.txt, &vocabulary) {
            Ok(tags) => {
                db.set_ai_tags(&note.rowid.into(), &tags)?;
                report.processed += 1;
            }
            Err(e) => {
                tracing::warn!(rowid = note.rowid, error = %e, "tagging failed");
                report.errors += 1;
            }
        }
    }
    progress(total, total);
    Ok(report)
}

/// Embed up to `limit` notes whose embeddings are missing or stale.
///
/// # Errors
/// See [`tag_notes`].
pub fn embed_notes(db: &mut Fastxt, limit: u32, progress: &dyn Fn(u32, u32)) -> Result<JobReport> {
    let ai = Ai::for_db(db)?;
    let check = ai.check();
    if !check.ok {
        return Err(Error::AiUnavailable(check.message));
    }
    let model_id = ai.embedding_model_id().to_string();
    let notes = db.notes_without_embedding(&model_id, limit)?;
    let total = notes.len() as u32;
    let mut report = JobReport::default();
    for (done, note) in notes.iter().enumerate() {
        progress(done as u32, total);
        match ai.embed(&note.txt) {
            Ok(vector) => {
                // Skips silently if the note was edited while we ran.
                db.store_embedding_if_current(note, &model_id, &vector)?;
                report.processed += 1;
            }
            Err(e) => {
                tracing::warn!(rowid = note.rowid, error = %e, "embedding failed");
                report.errors += 1;
            }
        }
    }
    progress(total, total);
    Ok(report)
}

/// Categorize up to `limit` notes that have no category, reusing existing
/// category names so renames stick.
///
/// # Errors
/// See [`tag_notes`].
pub fn categorize_notes(
    db: &mut Fastxt,
    limit: u32,
    progress: &dyn Fn(u32, u32),
) -> Result<JobReport> {
    let ai = Ai::for_db(db)?;
    let check = ai.check();
    if !check.ok {
        return Err(Error::AiUnavailable(check.message));
    }
    let existing: Vec<String> = db.categories()?.into_iter().map(|c| c.category).collect();
    let allowed = category_vocabulary(&existing);
    let notes = db.notes_without_category(limit)?;
    let total = notes.len() as u32;
    let mut report = JobReport::default();
    for (done, chunk) in notes.chunks(10).enumerate() {
        progress((done * 10) as u32, total);
        let texts: Vec<&str> = chunk.iter().map(|n| n.txt.as_str()).collect();
        match ai.categorize(&texts, &allowed) {
            Ok(categories) => {
                for (note, category) in chunk.iter().zip(categories.iter()) {
                    db.set_ai_category(&note.rowid.into(), Some(category))?;
                    report.processed += 1;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "categorizing a batch failed");
                report.errors += chunk.len() as u32;
            }
        }
    }
    progress(total, total);
    Ok(report)
}

/// An AI summary of arbitrary text without touching the database
/// (the desktop summarises the note it is still composing).
///
/// # Errors
/// See [`Ai::summarize`].
pub fn summarize_text(db: &Fastxt, text: &str) -> Result<String> {
    let ai = Ai::for_db(db)?;
    let check = ai.check();
    if !check.ok {
        return Err(Error::AiUnavailable(check.message));
    }
    ai.summarize(text)
}

/// Embed a query and run hybrid search with it.
///
/// # Errors
/// [`Error::AiUnavailable`] when the backend is down (callers may fall back
/// to text-only search); see also [`Fastxt::hybrid_search`].
pub fn hybrid_with_embeddings(
    db: &Fastxt,
    query: &str,
    limit: u32,
) -> Result<Vec<crate::model::ScoredNote>> {
    let ai = Ai::for_db(db)?;
    if !ai.is_available() {
        return Err(Error::AiUnavailable(ai.check().message));
    }
    let vector = ai.embed(query)?;
    db.hybrid_search(query, Some((&vector, ai.embedding_model_id())), limit)
}

/// Vector for `note`'s current text, or `None` when it has none.
#[must_use]
pub fn note_embedding(db: &Fastxt, note: &Note, model_id: &str) -> Option<Vec<f32>> {
    db.embedding(&note.rowid.into(), model_id).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NewNote;

    fn settings(rounds: u32) -> AiSettings {
        AiSettings {
            consistency_rounds: rounds,
            ..AiSettings::default()
        }
    }

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("hello", 100), "hello");
        let cjk = "你好世界"; // 12 bytes, 3 bytes per char
        assert_eq!(truncate(cjk, 7), "你好", "cuts back to the 6-byte boundary");
        assert_eq!(truncate(cjk, 6), "你好");
        assert_eq!(truncate(cjk, 5), "你");
        assert_eq!(truncate(cjk, 0), "");
    }

    #[test]
    fn extract_json_handles_fenced_and_nested_output() {
        assert_eq!(extract_json(r#"{"tags":["a"]}"#), Some(r#"{"tags":["a"]}"#));
        assert_eq!(
            extract_json("```json\n{\"tags\": [\"a {x} \"]}\n```"),
            Some(r#"{"tags": ["a {x} "]}"#)
        );
        assert_eq!(
            extract_json(r#"prefix {"a":{"b":"}]"} } suffix"#),
            Some(r#"{"a":{"b":"}]"} }"#)
        );
    }

    #[test]
    fn majority_vote_keeps_majority_in_first_seen_order() {
        let rounds = vec![
            vec!["rust".into(), "code".into(), "x1".into()],
            vec!["rust".into(), "code".into(), "x2".into()],
            vec!["rust".into(), "x3".into(), "code".into()],
        ];
        assert_eq!(majority_vote(rounds), vec!["rust", "code"]);
        // A tag first appearing in a later round still wins if it majorities.
        let rounds = vec![
            vec!["a".into()],
            vec!["a".into(), "b".into()],
            vec!["b".into()],
        ];
        assert_eq!(majority_vote(rounds), vec!["a", "b"]);
    }

    #[test]
    fn category_vocabulary_merges_existing_and_defaults() {
        let v = category_vocabulary(&["Job".into(), "other".into()]);
        assert_eq!(v.first().map(String::as_str), Some("job"));
        assert!(v.contains(&"work".to_string()));
        assert_eq!(v.iter().filter(|c| *c == "other").count(), 1);
    }

    #[test]
    fn mock_backend_powers_suggest_tags() {
        let ai = Ai::with_backend(Box::new(mock::MockBackend), settings(1));
        assert!(ai.is_available());
        let tags = ai
            .suggest_tags("rust async tokio note", &["rust".into()])
            .unwrap();
        assert!(!tags.is_empty());
    }

    #[test]
    fn jobs_fail_fast_when_the_backend_is_down() {
        let mut db = Fastxt::open_in_memory().unwrap();
        db.insert(NewNote::new("note", "")).unwrap();
        // Port 1 refuses connections everywhere, so this test never touches
        // a real Ollama even when one is running on the dev machine.
        db.save_settings(&crate::model::Settings {
            ai: AiSettings {
                endpoint: "http://127.0.0.1:1".into(),
                ..settings(1)
            },
        })
        .unwrap();
        let no_progress = |_, _| {};
        let result = tag_notes(&mut db, 10, &no_progress);
        assert!(matches!(result, Err(Error::AiUnavailable(_))), "{result:?}");
    }
}
