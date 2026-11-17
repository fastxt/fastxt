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

//! Fastxt as an MCP server (`fastxt mcp`): notes, search, AI, agent memory,
//! and the prompts the user prepared, most often dictated on their phone,
//! for an agent to take.
//!
//! An agent takes a prompt with the `take_prompt` tool when the user asks
//! it to, or the user hands one over with the `take` MCP prompt (in Claude
//! Code, `/mcp__fastxt__take` with the server added as `fastxt`), which puts
//! the text in as the user's own message. Either way the take is recorded
//! like a send ([`Sent::TAKEN`]): the prompt stops waiting, and the phone
//! shows where it went once it syncs.
//!
//! Every database and AI call runs through the typed `fastxt_core` API on a
//! blocking thread (`spawn_blocking`) — never on the async runtime, which the
//! blocking HTTP AI client would stall.

use fastxt_core::model::{Filter, NewNote, Note, NoteKey, Sent, TagCount};
use fastxt_core::sent::Take;
use fastxt_core::tags::{KIND_IDEA, KIND_PROMPT};
use fastxt_core::{Fastxt, SharedDb};
use rmcp::{
    ErrorData as McpError, Peer, RoleServer, ServerHandler, ServiceExt,
    handler::server::wrapper::Parameters, model::*, prompt, prompt_handler, prompt_router,
    schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

type DynResult<T> = Result<T, McpError>;

fn db_error(e: fastxt_core::Error) -> McpError {
    McpError {
        code: ErrorCode::INTERNAL_ERROR,
        message: format!("database error: {e}").into(),
        data: None,
    }
}

/// Run a blocking closure against the shared database.
async fn with_db<T, F>(db: &SharedDb, f: F) -> DynResult<T>
where
    T: Send + 'static,
    F: FnOnce(&mut Fastxt) -> Result<T, fastxt_core::Error> + Send + 'static,
{
    let db = db.clone();
    tokio::task::spawn_blocking(move || -> DynResult<T> {
        let mut guard = db.lock().map_err(|_| McpError {
            code: ErrorCode::INTERNAL_ERROR,
            message: "the database is locked by another operation".into(),
            data: None,
        })?;
        f(&mut guard).map_err(db_error)
    })
    .await
    .map_err(|e| McpError {
        code: ErrorCode::INTERNAL_ERROR,
        message: format!("the command panicked: {e}").into(),
        data: None,
    })?
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchNotesParams {
    /// Terms to search for in text, tags and AI tags.
    query: String,
    /// Maximum number of results (default 20).
    limit: Option<u32>,
    /// Number of results to skip (default 0).
    offset: Option<u32>,
    /// Restrict to one AI category.
    #[serde(default)]
    category: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SaveNoteParams {
    /// The note text.
    text: String,
    /// Comma-separated tags (a tag may contain spaces).
    tags: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct NoteIdParams {
    /// The note's rowid or uuid4.
    rowid: Option<i64>,
    uuid4: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct UpdateNoteParams {
    /// The note's rowid or uuid4.
    rowid: Option<i64>,
    uuid4: Option<String>,
    /// The new text.
    text: String,
    /// The new comma-separated tags.
    tags: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RelatedParams {
    /// The note's rowid or uuid4.
    rowid: Option<i64>,
    uuid4: Option<String>,
    /// Maximum number of related notes (default 5).
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TagTextParams {
    /// Text to suggest tags for.
    text: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RememberParams {
    /// The text to remember.
    text: String,
    /// Comma-separated tags.
    tags: Option<String>,
    /// Agent identifier, stored as an `agent:<id>` tag.
    agent_id: Option<String>,
    /// Topic, stored as a `topic:<name>` tag (may contain spaces).
    topic: Option<String>,
    /// low, medium or high (default medium).
    importance: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RecallParams {
    /// Terms to search for.
    query: String,
    /// Only memories from this agent.
    agent_id: Option<String>,
    /// Only memories with this topic.
    topic: Option<String>,
    /// Maximum number of results (default 10).
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SummarizeTopicParams {
    /// Topic whose memories should be summarized.
    topic: String,
    /// Maximum notes to include (default 20).
    max_notes: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WaitingParams {
    /// `prompt` (the default) or `idea`.
    kind: Option<String>,
    /// Words the text must have.
    query: Option<String>,
    /// Also list the ones already typed into a session or taken by an agent,
    /// with where they went.
    include_sent: Option<bool>,
    /// Maximum number of results (default 20).
    limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TakeParams {
    /// The note's rowid, as waiting_prompts lists it, or its uuid4. Without
    /// either, the newest waiting prompt.
    rowid: Option<i64>,
    uuid4: Option<String>,
    /// Without rowid or uuid4: words from the waiting prompt to take.
    query: Option<String>,
    /// Without rowid or uuid4: `prompt` (the default) or `idea`.
    kind: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct TakeArgs {
    /// Words from the prompt, or its number. Empty: the newest one waiting.
    note: Option<String>,
}

/// The tag of a kind of note, as people name the kind.
fn kind_tag(kind: Option<&str>) -> DynResult<&'static str> {
    match kind.map(|k| k.trim().to_lowercase()).as_deref() {
        None | Some("" | "prompt" | "prompts") => Ok(KIND_PROMPT),
        Some("idea" | "ideas") => Ok(KIND_IDEA),
        Some(other) => Err(McpError {
            code: ErrorCode::INVALID_PARAMS,
            message: format!("kind is prompt or idea, not {other:?}").into(),
            data: None,
        }),
    }
}

/// Who takes a note: the agent, as its MCP client names itself, and the
/// folder this server runs in, which is the one the agent started it in.
fn taker(peer: &Peer<RoleServer>) -> (String, String) {
    let agent = peer
        .peer_info()
        .map(|info| {
            let client = &info.client_info;
            client
                .title
                .as_deref()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| client.name.trim())
                .to_string()
        })
        .filter(|agent| !agent.is_empty())
        .unwrap_or_else(|| "agent".to_string());
    let cwd = std::env::current_dir()
        .map(|dir| dir.display().to_string())
        .unwrap_or_default();
    (agent, cwd)
}

/// The start of a note's text, for a list.
fn preview(text: &str) -> String {
    const WIDTH: usize = 200;
    if text.chars().count() <= WIDTH {
        text.to_string()
    } else {
        let kept: String = text.chars().take(WIDTH - 1).collect();
        format!("{kept}…")
    }
}

/// A note's tags apart from its kind.
fn shown_tags(note: &Note) -> Vec<String> {
    note.tag_list()
        .into_iter()
        .filter(|tag| !tag.to_lowercase().starts_with("kind:"))
        .collect()
}

/// Why nothing was taken from the waiting notes of a kind.
fn none_waiting(tag: &str, words: &str) -> String {
    let what = if tag == KIND_IDEA { "idea" } else { "prompt" };
    let words = words.trim();
    if words.is_empty() {
        format!(
            "No {what} is waiting. They come from the Fastxt app, most often dictated on the \
             phone, and reach this computer when the phone syncs with it: on the phone, \
             Computer, then Sync notes now, while `fastxt serve` runs here."
        )
    } else {
        format!("No waiting {what} has {words:?}.")
    }
}

/// Whether text is a note's number or uuid rather than words from it.
fn is_note_id(given: &str) -> bool {
    let uuid = given.len() == 36
        && given.chars().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        });
    uuid || given.parse::<i64>().is_ok()
}

/// A note named by its number (rowid) or uuid.
fn note_key(given: &str) -> NoteKey {
    given
        .parse::<i64>()
        .map_or_else(|_| NoteKey::Uuid(given.to_string()), NoteKey::Rowid)
}

/// The note to take: a given one, or one of those waiting.
enum Which {
    Key(NoteKey),
    Waiting { tag: &'static str, words: String },
}

/// Take a note for the agent behind `peer`; when none matches, why.
async fn take_for(
    db: &SharedDb,
    which: Which,
    peer: &Peer<RoleServer>,
) -> DynResult<Result<(Note, Sent), String>> {
    let (agent, cwd) = taker(peer);
    with_db(db, move |db| {
        let taken = match &which {
            Which::Key(key) => db.take(Take::Note(key), &agent, &cwd)?,
            Which::Waiting { tag, words } => db.take(Take::Waiting { tag, words }, &agent, &cwd)?,
        };
        Ok(taken.ok_or_else(|| match &which {
            Which::Key(key) => format!("there is no note with {key}"),
            Which::Waiting { tag, words } => none_waiting(tag, words),
        }))
    })
    .await
}

fn key_of(rowid: Option<i64>, uuid4: Option<String>) -> DynResult<NoteKey> {
    match (rowid, uuid4) {
        (Some(rowid), _) => Ok(NoteKey::Rowid(rowid)),
        (None, Some(uuid4)) => Ok(NoteKey::Uuid(uuid4)),
        (None, None) => Err(McpError {
            code: ErrorCode::INVALID_PARAMS,
            message: "pass rowid or uuid4".into(),
            data: None,
        }),
    }
}

fn agent_tags(
    user_tags: Option<&str>,
    agent_id: Option<&str>,
    topic: Option<&str>,
    importance: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(agent) = agent_id.filter(|a| !a.trim().is_empty()) {
        parts.push(format!("agent:{agent}"));
    }
    if let Some(topic) = topic.filter(|t| !t.trim().is_empty()) {
        parts.push(format!("topic:{topic}"));
    }
    let importance = importance
        .map(str::trim)
        .filter(|i| matches!(i.to_lowercase().as_str(), "low" | "medium" | "high"))
        .unwrap_or("medium");
    parts.push(format!("importance:{importance}"));
    if let Some(tags) = user_tags.filter(|t| !t.trim().is_empty()) {
        parts.push(tags.to_string());
    }
    parts.join(",")
}

/// The Fastxt MCP server.
#[derive(Clone)]
struct FastxtMcpServer {
    db: SharedDb,
}

impl FastxtMcpServer {
    fn new(db: Fastxt) -> Self {
        FastxtMcpServer {
            db: Arc::new(std::sync::Mutex::new(db)),
        }
    }
}

#[tool_router]
impl FastxtMcpServer {
    #[tool(
        name = "search_notes",
        description = "Search notes by text. Terms match note text, tags and AI tags; all terms must match. Empty query lists recent notes. Optionally restricted to one AI category."
    )]
    async fn search_notes(
        &self,
        Parameters(params): Parameters<SearchNotesParams>,
    ) -> DynResult<String> {
        let query = params.query;
        let limit = params.limit.unwrap_or(20);
        let offset = params.offset.unwrap_or(0);
        let filter = Filter {
            category: params.category,
            ..Filter::default()
        };
        with_db(&self.db, move |db| {
            let page = db.search(&query, limit, offset, &filter)?;
            serde_json::to_string(&page).map_err(fastxt_core::Error::from)
        })
        .await
    }

    #[tool(
        name = "list_notes",
        description = "List recent notes, newest first, optionally filtered by AI category."
    )]
    async fn list_notes(
        &self,
        Parameters(params): Parameters<SearchNotesParams>,
    ) -> DynResult<String> {
        let limit = params.limit.unwrap_or(20);
        let offset = params.offset.unwrap_or(0);
        let filter = Filter {
            category: params.category,
            ..Filter::default()
        };
        with_db(&self.db, move |db| {
            let page = db.list(limit, offset, &filter)?;
            serde_json::to_string(&page).map_err(fastxt_core::Error::from)
        })
        .await
    }

    #[tool(
        name = "get_note",
        description = "Fetch one note by rowid or uuid4, including AI tags, summary and category."
    )]
    async fn get_note(&self, Parameters(params): Parameters<NoteIdParams>) -> DynResult<String> {
        let key = key_of(params.rowid, params.uuid4)?;
        with_db(&self.db, move |db| match db.get(&key)? {
            Some(note) => serde_json::to_string(&note).map_err(fastxt_core::Error::from),
            None => Err(fastxt_core::Error::NotFound(format!("{key}"))),
        })
        .await
    }

    #[tool(
        name = "save_note",
        description = "Save a new note; returns the created note with its rowid and uuid4."
    )]
    async fn save_note(&self, Parameters(params): Parameters<SaveNoteParams>) -> DynResult<String> {
        let new = NewNote::new(params.text, params.tags.unwrap_or_default());
        with_db(&self.db, move |db| {
            let note = db.insert(new)?;
            serde_json::to_string(&note).map_err(fastxt_core::Error::from)
        })
        .await
    }

    #[tool(
        name = "update_note",
        description = "Change a note's text and tags by rowid or uuid4; returns the updated note."
    )]
    async fn update_note(
        &self,
        Parameters(params): Parameters<UpdateNoteParams>,
    ) -> DynResult<String> {
        let key = key_of(params.rowid, params.uuid4)?;
        let text = params.text;
        let tags = params.tags.unwrap_or_default();
        with_db(&self.db, move |db| {
            let note = db.update(&key, &text, &tags)?;
            serde_json::to_string(&note).map_err(fastxt_core::Error::from)
        })
        .await
    }

    #[tool(
        name = "delete_note",
        description = "Delete a note by rowid or uuid4. The deletion syncs to other devices."
    )]
    async fn delete_note(&self, Parameters(params): Parameters<NoteIdParams>) -> DynResult<String> {
        let key = key_of(params.rowid, params.uuid4)?;
        with_db(&self.db, move |db| {
            let deleted = db.delete(&key)?;
            Ok(format!(r#"{{"deleted":{deleted}}}"#))
        })
        .await
    }

    #[tool(
        name = "suggest_tags",
        description = "Suggest tags for arbitrary text using the on-device AI model. Requires the AI backend (e.g. Ollama) to be running."
    )]
    async fn suggest_tags(
        &self,
        Parameters(params): Parameters<TagTextParams>,
    ) -> DynResult<String> {
        let text = params.text;
        with_db(&self.db, move |db| {
            let vocabulary: Vec<String> = db
                .tag_vocabulary(40)?
                .into_iter()
                .map(|t: TagCount| t.tag)
                .collect();
            let ai = fastxt_core::ai::Ai::for_db(db)?;
            match ai.suggest_tags(&text, &vocabulary) {
                Ok(tags) => Ok(serde_json::json!({ "tags": tags, "available": true }).to_string()),
                Err(e) => Ok(
                    serde_json::json!({ "tags": [], "available": false, "error": e.to_string() })
                        .to_string(),
                ),
            }
        })
        .await
    }

    #[tool(
        name = "summarize_text",
        description = "Summarize arbitrary text with the on-device AI model. Nothing is saved."
    )]
    async fn summarize_text(
        &self,
        Parameters(params): Parameters<TagTextParams>,
    ) -> DynResult<String> {
        let text = params.text;
        with_db(&self.db, move |db| {
            match fastxt_core::ai::summarize_text(db, &text) {
                Ok(summary) => {
                    Ok(serde_json::json!({ "summary": summary, "available": true }).to_string())
                }
                Err(e) => Ok(serde_json::json!({
                    "summary": null, "available": false, "error": e.to_string()
                })
                .to_string()),
            }
        })
        .await
    }

    #[tool(
        name = "find_related",
        description = "Find notes similar to a given note by embedding similarity. Requires embeddings (run ai-embed-all first)."
    )]
    async fn find_related(
        &self,
        Parameters(params): Parameters<RelatedParams>,
    ) -> DynResult<String> {
        let key = key_of(params.rowid, params.uuid4)?;
        let limit = params.limit.unwrap_or(5);
        with_db(&self.db, move |db| {
            let hits = db.related(&key, None, limit)?;
            let notes: Vec<_> = hits
                .into_iter()
                .map(|h| serde_json::json!({ "note": h.note, "similarity": h.score }))
                .collect();
            Ok(serde_json::json!({ "results": notes }).to_string())
        })
        .await
    }

    #[tool(
        name = "get_categories",
        description = "List AI categories with their note counts."
    )]
    async fn get_categories(&self) -> DynResult<String> {
        with_db(&self.db, move |db| {
            let categories = db.categories()?;
            Ok(serde_json::json!({ "categories": categories }).to_string())
        })
        .await
    }

    #[tool(
        name = "ai_status",
        description = "Report whether the on-device AI backend is reachable, with setup hints."
    )]
    async fn ai_status(&self) -> DynResult<String> {
        with_db(&self.db, move |db| {
            let check = fastxt_core::ai::Ai::for_db(db)?.check();
            Ok(serde_json::json!({
                "ok": check.ok,
                "message": check.message,
                "settings": db.settings()?,
            })
            .to_string())
        })
        .await
    }

    // ---- prompts the user prepared ---------------------------------------

    #[tool(
        name = "waiting_prompts",
        description = "List the prompts the user prepared in Fastxt, most often dictated on their phone, that wait for an agent: never typed into a session nor taken. Newest first, with the start of each text; created_at is UTC. kind=idea lists their ideas instead; include_sent=true adds the ones already sent or taken, with where they went. When the user asks you to work on one, take it with take_prompt."
    )]
    async fn waiting_prompts(
        &self,
        Parameters(params): Parameters<WaitingParams>,
    ) -> DynResult<String> {
        let tag = kind_tag(params.kind.as_deref())?;
        let query = params.query.unwrap_or_default();
        let limit = params.limit.unwrap_or(20);
        let include_sent = params.include_sent.unwrap_or(false);
        let filter = Filter {
            tags: vec![tag.to_string()],
            unsent: !include_sent,
            ..Filter::default()
        };
        with_db(&self.db, move |db| {
            let page = db.search(&query, limit, 0, &filter)?;
            let mut prompts = Vec::with_capacity(page.notes.len());
            for note in &page.notes {
                let mut listed = json!({
                    "rowid": note.rowid,
                    "uuid4": note.uuid4,
                    "created_at": note.created_at,
                    "text": preview(&note.txt),
                    "tags": shown_tags(note),
                });
                if include_sent {
                    listed["sent"] = serde_json::to_value(db.sent(Some(&note.uuid4), 20)?)?;
                }
                prompts.push(listed);
            }
            let mut out = json!({ "count": page.count, "prompts": prompts });
            if page.count == 0 && !include_sent {
                out["message"] = json!(none_waiting(tag, &query));
            }
            Ok(out.to_string())
        })
        .await
    }

    #[tool(
        name = "take_prompt",
        description = "Take one of the prompts the user prepared, when they ask you to: returns its whole text, which they wrote for an agent to carry out, and records that you took it, so it stops waiting and their phone shows where it went. Without rowid or uuid4, takes the newest waiting prompt, or the waiting one that query's words find; kind=idea takes an idea instead."
    )]
    async fn take_prompt(
        &self,
        Parameters(params): Parameters<TakeParams>,
        peer: Peer<RoleServer>,
    ) -> DynResult<String> {
        let which = match (params.rowid, params.uuid4) {
            (None, None) => Which::Waiting {
                tag: kind_tag(params.kind.as_deref())?,
                words: params.query.unwrap_or_default(),
            },
            (rowid, uuid4) => Which::Key(key_of(rowid, uuid4)?),
        };
        let given = matches!(which, Which::Key(_));
        match take_for(&self.db, which, &peer).await? {
            Ok((note, sent)) => Ok(json!({
                "text": note.txt,
                "note": {
                    "rowid": note.rowid,
                    "uuid4": note.uuid4,
                    "created_at": note.created_at,
                    "tags": shown_tags(&note),
                },
                "taken": sent,
            })
            .to_string()),
            Err(message) if given => Err(McpError {
                code: ErrorCode::INVALID_PARAMS,
                message: message.into(),
                data: None,
            }),
            Err(message) => Ok(json!({ "text": null, "message": message }).to_string()),
        }
    }

    // ---- agent memory ---------------------------------------------------

    #[tool(
        name = "remember",
        description = "Store a memory as a note with structured metadata tags (agent id, topic, importance). Returns the created note."
    )]
    async fn remember(&self, Parameters(params): Parameters<RememberParams>) -> DynResult<String> {
        let tags = agent_tags(
            params.tags.as_deref(),
            params.agent_id.as_deref(),
            params.topic.as_deref(),
            params.importance.as_deref(),
        );
        let text = params.text;
        with_db(&self.db, move |db| {
            let note = db.insert(NewNote::new(text, tags))?;
            serde_json::to_string(&note).map_err(fastxt_core::Error::from)
        })
        .await
    }

    #[tool(
        name = "recall",
        description = "Search memories; optionally filter by the agent id and topic they were stored with."
    )]
    async fn recall(&self, Parameters(params): Parameters<RecallParams>) -> DynResult<String> {
        let query = params.query;
        let limit = params.limit.unwrap_or(10);
        let agent = params
            .agent_id
            .filter(|a| !a.trim().is_empty())
            .map(|a| format!("agent:{a}"));
        let topic = params
            .topic
            .filter(|t| !t.trim().is_empty())
            .map(|t| format!("topic:{t}"));
        // Exact tag membership, applied in SQL so the count is real.
        let filter = Filter {
            tags: [agent, topic].into_iter().flatten().collect(),
            ..Filter::default()
        };
        with_db(&self.db, move |db| {
            let page = db.search(&query, limit, 0, &filter)?;
            Ok(serde_json::json!({ "notes": page.notes, "count": page.count }).to_string())
        })
        .await
    }

    #[tool(
        name = "forget",
        description = "Delete a memory by its note rowid or uuid4."
    )]
    async fn forget(&self, Parameters(params): Parameters<NoteIdParams>) -> DynResult<String> {
        let key = key_of(params.rowid, params.uuid4)?;
        with_db(&self.db, move |db| {
            let deleted = db.delete(&key)?;
            Ok(format!(r#"{{"deleted":{deleted}}}"#))
        })
        .await
    }

    #[tool(
        name = "list_topics",
        description = "List topics used across memories with their counts."
    )]
    async fn list_topics(&self) -> DynResult<String> {
        with_db(&self.db, move |db| {
            // Tag counts come from the tags column alone: no note bodies.
            let mut topics: Vec<(String, u32)> = db
                .user_tag_vocabulary(usize::MAX)?
                .into_iter()
                .filter_map(|t| {
                    t.tag
                        .strip_prefix("topic:")
                        .map(|topic| (topic.to_string(), t.count))
                })
                .collect();
            topics.sort();
            let topics: Vec<_> = topics
                .into_iter()
                .map(|(topic, count)| serde_json::json!({ "topic": topic, "count": count }))
                .collect();
            Ok(serde_json::json!({ "topics": topics }).to_string())
        })
        .await
    }

    #[tool(
        name = "summarize_topic",
        description = "Summarize all memories on a topic with the on-device AI. Nothing is saved; no temporary notes are created."
    )]
    async fn summarize_topic(
        &self,
        Parameters(params): Parameters<SummarizeTopicParams>,
    ) -> DynResult<String> {
        let topic = params.topic;
        let max_notes = params.max_notes.unwrap_or(20);
        with_db(&self.db, move |db| {
            // Exact tag membership, not a text search for "topic:…".
            let filter = Filter {
                tags: vec![format!("topic:{topic}")],
                ..Filter::default()
            };
            let page = db.list(max_notes, 0, &filter)?;
            let texts: Vec<&str> = page.notes.iter().map(|n| n.txt.as_str()).collect();
            if texts.is_empty() {
                return Ok(serde_json::json!({
                    "summary": null, "note_count": 0,
                    "error": "no memories for this topic"
                })
                .to_string());
            }
            let combined = texts.join("\n\n---\n\n");
            match fastxt_core::ai::summarize_text(db, &combined) {
                Ok(summary) => Ok(serde_json::json!({
                    "summary": summary, "note_count": texts.len()
                })
                .to_string()),
                Err(e) => Ok(serde_json::json!({
                    "summary": null, "note_count": texts.len(), "error": e.to_string()
                })
                .to_string()),
            }
        })
        .await
    }
}

#[prompt_router]
impl FastxtMcpServer {
    #[prompt(
        name = "take",
        description = "Your newest prompt from Fastxt that no agent has taken, or the one your words find, as your message here. It is marked as taken, on your phone too."
    )]
    async fn take_message(
        &self,
        Parameters(args): Parameters<TakeArgs>,
        peer: Peer<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let which = match args.note.as_deref().map(str::trim).unwrap_or_default() {
            given if is_note_id(given) => Which::Key(note_key(given)),
            words => Which::Waiting {
                tag: KIND_PROMPT,
                words: words.to_string(),
            },
        };
        match take_for(&self.db, which, &peer).await? {
            // Only the text: it goes in as if the user had typed it here.
            Ok((note, _)) => Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                Role::User,
                note.txt,
            )])),
            Err(message) => Err(McpError {
                code: ErrorCode::INVALID_PARAMS,
                message: message.into(),
                data: None,
            }),
        }
    }
}

#[tool_handler]
#[prompt_handler]
impl ServerHandler for FastxtMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(
            Implementation::new("fastxt-mcp", env!("CARGO_PKG_VERSION")).with_description(
                "Fastxt local-first notes with on-device AI, agent memory, and the prompts you prepared on your phone",
            ),
        )
        .with_instructions(
            "Fastxt note store.\n\n\
             Notes: search_notes, list_notes, get_note, save_note, update_note, delete_note.\n\
             Prompts the user prepared, most often dictated on their phone: waiting_prompts \
             lists the ones no agent has had yet; take_prompt gives you one to work on and \
             records that you took it, so it stops waiting. Take one only when the user asks. \
             The `take` prompt hands the user their newest one as their own message.\n\
             AI (needs a local backend, see ai_status): suggest_tags, summarize_text, \
             find_related, get_categories.\n\
             Agent memory: remember, recall, forget, list_topics, summarize_topic — \
             memories are notes with agent:/topic:/importance: tags; filter recall by \
             agent_id or topic.",
        )
    }
}

/// Serve MCP on stdin and stdout, over `db`, until the client hangs up.
/// Logs go to stderr: stdout carries the protocol.
///
/// # Errors
/// When the transport fails.
pub fn serve(db: Fastxt) -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            tracing::info!("starting Fastxt MCP server");
            let service = FastxtMcpServer::new(db)
                .serve((tokio::io::stdin(), tokio::io::stdout()))
                .await
                .inspect_err(|e| tracing::error!("serving error: {e:?}"))?;
            service.waiting().await?;
            Ok(())
        })
}
