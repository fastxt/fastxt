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

//! Framework-agnostic business logic for the Fastxt desktop app.
//!
//! Every public function here is a thin, **synchronous** wrapper over the
//! [`fastxt_core::exe::run`] JSON action contract. There are deliberately no GUI
//! types in this module — the Iced layer ([`crate::app`]) calls these from a
//! background thread via [`tokio::task::spawn_blocking`], so they must stay pure
//! and `Send`. Keeping them here (and not in the view) is what makes the GUI
//! framework swappable: the same logic backed Druid and now backs Iced.

use serde_json::{Value, json};

/// A single note rendered as a card in the list / search results.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoteCard {
    pub rowid: i64,
    pub txt: String,
    pub tags: String,
    pub summary: String,
    pub created_at: String,
    /// Similarity score in `[0, 1]` for semantic search; `None` for text search.
    pub similarity: Option<f64>,
}

/// Result of a search / list operation.
#[derive(Debug, Clone, Default)]
pub struct SearchOutcome {
    pub count_label: String,
    pub notes: Vec<NoteCard>,
    /// Non-empty when the backend reported a problem (e.g. AI unavailable).
    pub status: String,
}

/// AI tag suggestion plus a human-readable status line.
#[derive(Debug, Clone, Default)]
pub struct TagSuggestion {
    pub tags: String,
    pub status: String,
}

/// AI summary plus a human-readable status line.
#[derive(Debug, Clone, Default)]
pub struct SummaryOutcome {
    pub summary: String,
    pub status: String,
}

/// Run a JSON command through the core and parse the response as JSON.
fn run(cmd: &Value) -> Value {
    let result = fastxt_core::exe::run(&cmd.to_string());
    serde_json::from_str(&result).unwrap_or_else(|_| json!({ "error": "failed to parse response" }))
}

/// Read a string field from a JSON object, defaulting to `""`.
fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Build a [`NoteCard`] from a note JSON object, keeping only the date portion
/// of `created_at` (the core stores `"%Y-%m-%d %H:%M:%S"`).
fn note_from_json(note: &Value, similarity: Option<f64>) -> NoteCard {
    let created_at = str_field(note, "created_at")
        .split(' ')
        .next()
        .unwrap_or("")
        .to_string();
    NoteCard {
        rowid: note.get("rowid").and_then(Value::as_i64).unwrap_or(0),
        txt: str_field(note, "txt"),
        tags: str_field(note, "tags"),
        summary: str_field(note, "ai_summary"),
        created_at,
        similarity,
    }
}

/// Format a `{processed, errors}` batch response, or surface its error.
fn batch_status(resp: &Value, verb: &str) -> String {
    if let Some(processed) = resp.get("processed").and_then(Value::as_u64) {
        let errors = resp.get("errors").and_then(Value::as_u64).unwrap_or(0);
        format!("{verb} {processed} notes ({errors} errors)")
    } else {
        let err = str_field(resp, "error");
        if err.is_empty() {
            format!("Failed: {verb}")
        } else {
            format!("Error: {err}")
        }
    }
}

/// List the most recent notes (used to populate the list on startup).
pub fn recent_notes(limit: u32, offset: u32) -> SearchOutcome {
    let resp = run(&json!({ "action": "select", "limit": limit, "offset": offset }));
    let notes: Vec<NoteCard> = resp
        .get("notes")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(|n| note_from_json(n, None)).collect())
        .unwrap_or_default();
    let count = resp
        .get("count")
        .and_then(Value::as_u64)
        .unwrap_or(notes.len() as u64);
    SearchOutcome {
        count_label: format!("{count} notes"),
        notes,
        status: String::new(),
    }
}

/// Full-text search.
pub fn text_search(query: &str, limit: u32, offset: u32) -> SearchOutcome {
    let resp = run(&json!({
        "action": "search",
        "query": query,
        "limit": limit,
        "offset": offset,
    }));
    let count = resp.get("count").and_then(Value::as_u64).unwrap_or(0);
    let notes: Vec<NoteCard> = resp
        .get("notes")
        .and_then(Value::as_array)
        .map(|arr| arr.iter().map(|n| note_from_json(n, None)).collect())
        .unwrap_or_default();
    SearchOutcome {
        count_label: format!("{count} notes found"),
        notes,
        status: String::new(),
    }
}

/// AI-powered semantic search.
pub fn semantic_search(query: &str, endpoint: &str, model: &str) -> SearchOutcome {
    let resp = run(&json!({
        "action": "semantic-search",
        "query": query,
        "limit": 20,
        "threshold": 0.5,
        "endpoint": endpoint,
        "model": model,
    }));
    if let Some(results) = resp.get("results").and_then(Value::as_array) {
        let available = resp
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let notes: Vec<NoteCard> = results
            .iter()
            .filter_map(|r| {
                r.get("note")
                    .map(|note| note_from_json(note, r.get("similarity").and_then(Value::as_f64)))
            })
            .collect();
        SearchOutcome {
            count_label: format!("{} similar notes", notes.len()),
            notes,
            status: if available {
                String::new()
            } else {
                "AI not available for semantic search".to_string()
            },
        }
    } else {
        let err = str_field(&resp, "error");
        SearchOutcome {
            count_label: "0 notes".to_string(),
            notes: Vec::new(),
            status: if err.is_empty() {
                "Search failed".to_string()
            } else {
                format!("Error: {err}")
            },
        }
    }
}

/// Suggest tags for the given text via AI.
pub fn ai_tags(text: &str, endpoint: &str, model: &str) -> TagSuggestion {
    let resp = run(&json!({
        "action": "ai-tag",
        "text": text,
        "endpoint": endpoint,
        "model": model,
    }));
    if let Some(tags) = resp.get("tags").and_then(Value::as_array) {
        let tag_str = tags
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let available = resp
            .get("available")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        TagSuggestion {
            tags: tag_str,
            status: if available {
                String::new()
            } else {
                "AI not available. Check Ollama is running.".to_string()
            },
        }
    } else {
        let err = str_field(&resp, "error");
        TagSuggestion {
            tags: String::new(),
            status: if err.is_empty() {
                "Failed to parse AI response".to_string()
            } else {
                format!("Error: {err}")
            },
        }
    }
}

/// Insert a note. Returns the rowid of the inserted note, or `None` on failure.
pub fn insert_note(content: &str, tags: &str) -> Option<i64> {
    let resp = run(&json!({
        "action": "insert",
        "txt": content,
        "tags": tags,
        "limit": 1,
        "offset": 0,
    }));
    resp.get("notes")
        .and_then(Value::as_array)
        .and_then(|n| n.first())
        .and_then(|note| note.get("rowid").and_then(Value::as_i64))
}

/// Insert the current note, then ask AI to summarize it (mirrors the original
/// Druid flow, which persisted the note to obtain a rowid for `ai-summarize`).
pub fn summarize_new_note(
    content: &str,
    tags: &str,
    endpoint: &str,
    model: &str,
) -> SummaryOutcome {
    let Some(rowid) = insert_note(content, tags) else {
        return SummaryOutcome {
            summary: String::new(),
            status: "Failed to generate summary".to_string(),
        };
    };
    let resp = run(&json!({
        "action": "ai-summarize",
        "rowid": rowid,
        "endpoint": endpoint,
        "model": model,
    }));
    if let Some(summary) = resp.get("summary").and_then(Value::as_str) {
        SummaryOutcome {
            summary: summary.to_string(),
            status: String::new(),
        }
    } else {
        let err = str_field(&resp, "error");
        SummaryOutcome {
            summary: String::new(),
            status: if err.is_empty() {
                "Failed to generate summary".to_string()
            } else {
                format!("Error: {err}")
            },
        }
    }
}

/// Probe the configured AI endpoint with a trivial request.
pub fn test_connection(endpoint: &str, model: &str) -> String {
    let resp = run(&json!({
        "action": "ai-tag",
        "text": "test",
        "endpoint": endpoint,
        "model": model,
    }));
    if resp
        .get("available")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "✓ Connected to Ollama successfully!".to_string()
    } else {
        "✗ Could not connect to Ollama. Make sure it's running.".to_string()
    }
}

/// Batch-tag all notes that lack AI tags.
pub fn tag_all(endpoint: &str, model: &str) -> String {
    let resp = run(&json!({
        "action": "ai-tag-all",
        "limit": 100,
        "endpoint": endpoint,
        "model": model,
    }));
    batch_status(&resp, "Tagged")
}

/// Batch-embed all notes that lack embeddings.
pub fn embed_all(endpoint: &str, model: &str) -> String {
    let resp = run(&json!({
        "action": "ai-embed-all",
        "limit": 100,
        "endpoint": endpoint,
        "model": model,
    }));
    batch_status(&resp, "Embedded")
}

/// Ask AI to organize notes into categories, returning a status + breakdown.
pub fn organize(endpoint: &str, model: &str) -> String {
    let resp = run(&json!({
        "action": "ai-organize",
        "limit": 100,
        "endpoint": endpoint,
        "model": model,
    }));
    if let Some(processed) = resp.get("processed").and_then(Value::as_u64) {
        let errors = resp.get("errors").and_then(Value::as_u64).unwrap_or(0);
        let categories = resp
            .get("categories")
            .and_then(Value::as_object)
            .map(|obj| {
                obj.iter()
                    .map(|(k, v)| format!("  {}: {} notes", k, v.as_u64().unwrap_or(0)))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        format!("Organized {processed} notes ({errors} errors)\n{categories}")
    } else {
        let err = str_field(&resp, "error");
        if err.is_empty() {
            "Failed to organize notes".to_string()
        } else {
            format!("Error: {err}")
        }
    }
}

/// Sync against another Fastxt instance acting as server (one-shot client sync).
pub fn client_sync(addr: &str) -> String {
    let resp = run(&json!({ "action": "client-sync", "addr": addr }));
    if let Some(resp_str) = resp.get("client-sync").and_then(Value::as_str) {
        format!("✓ {resp_str}")
    } else {
        let err = str_field(&resp, "error");
        if err.is_empty() {
            "Sync failed".to_string()
        } else {
            format!("Error: {err}")
        }
    }
}

/// Load all notes grouped by AI category, formatted for display.
pub fn load_categories() -> String {
    use std::collections::BTreeMap;

    let resp = run(&json!({ "action": "select", "limit": 1000, "offset": 0 }));
    let Some(notes) = resp.get("notes").and_then(Value::as_array) else {
        return "Failed to load categories".to_string();
    };

    // BTreeMap keeps category order stable across runs (HashMap did not).
    let mut categories: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for note in notes {
        let category = note
            .get("ai_category")
            .and_then(Value::as_str)
            .unwrap_or("uncategorized")
            .to_string();
        let txt: String = str_field(note, "txt").chars().take(50).collect();
        categories.entry(category).or_default().push(txt);
    }

    if categories.is_empty() {
        return "No notes found. Run 'Organize Notes' to categorize.".to_string();
    }

    let mut parts = Vec::new();
    for (category, items) in &categories {
        parts.push(format!("📁 {} ({} notes)", category, items.len()));
        for item in items.iter().take(5) {
            parts.push(format!("  • {item}"));
        }
        if items.len() > 5 {
            parts.push(format!("  ... and {} more", items.len() - 5));
        }
        parts.push(String::new());
    }
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_note_fields_and_truncates_date_to_day() {
        let v = json!({
            "rowid": 7,
            "txt": "hello world",
            "tags": "a,b",
            "ai_summary": "a summary",
            "created_at": "2026-06-20 10:11:12",
        });
        let card = note_from_json(&v, Some(0.42));
        assert_eq!(card.rowid, 7);
        assert_eq!(card.txt, "hello world");
        assert_eq!(card.tags, "a,b");
        assert_eq!(card.summary, "a summary");
        assert_eq!(card.created_at, "2026-06-20");
        assert_eq!(card.similarity, Some(0.42));
    }

    #[test]
    fn note_defaults_when_fields_missing() {
        let card = note_from_json(&json!({}), None);
        assert_eq!(card.rowid, 0);
        assert_eq!(card.txt, "");
        assert_eq!(card.created_at, "");
        assert_eq!(card.similarity, None);
    }

    #[test]
    fn batch_status_reports_counts() {
        let v = json!({ "processed": 3, "errors": 1 });
        assert_eq!(batch_status(&v, "Tagged"), "Tagged 3 notes (1 errors)");
    }

    #[test]
    fn batch_status_surfaces_error() {
        let v = json!({ "error": "boom" });
        assert_eq!(batch_status(&v, "Tagged"), "Error: boom");
    }
}
