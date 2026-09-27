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

//! Fastxt as an MCP server: notes, search, AI and agent memory.
//!
//! Every database and AI call runs through the typed `fastxt_core` API on a
//! blocking thread (`spawn_blocking`) — never on the async runtime, which the
//! blocking HTTP AI client would stall.

use fastxt_core::model::{Filter, NewNote, NoteKey, TagCount};
use fastxt_core::{Fastxt, SharedDb};
use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, handler::server::wrapper::Parameters,
    model::*, schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;
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

#[tool_router]
impl FastxtMcpServer {
    fn new() -> DynResult<Self> {
        let db = Fastxt::open_default().map_err(db_error)?;
        Ok(FastxtMcpServer {
            db: Arc::new(std::sync::Mutex::new(db)),
        })
    }

    #[tool(
        name = "search_notes",
        description = "Search notes by text. Terms match note text, tags and AI tags; all terms must match. Empty query lists recent notes."
    )]
    async fn search_notes(
        &self,
        Parameters(params): Parameters<SearchNotesParams>,
    ) -> DynResult<String> {
        let query = params.query;
        let limit = params.limit.unwrap_or(20);
        let offset = params.offset.unwrap_or(0);
        let category = params.category;
        with_db(&self.db, move |db| {
            let mut page = db.search(&query, limit, offset)?;
            if let Some(category) = category {
                page.notes
                    .retain(|n| n.ai_category.as_deref() == Some(category.as_str()));
                page.count = page.notes.len() as u32;
            }
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
        let agent = params.agent_id.map(|a| format!("agent:{a}"));
        let topic = params.topic.map(|t| format!("topic:{t}"));
        with_db(&self.db, move |db| {
            // Fetch more than asked, filter on exact tag membership, re-limit.
            let page = db.search(&query, limit.saturating_mul(5), 0)?;
            let notes: Vec<_> = page
                .notes
                .into_iter()
                .filter(|note| {
                    let tags = note.tag_list();
                    let agent_ok = agent.as_ref().is_none_or(|a| tags.contains(a));
                    let topic_ok = topic.as_ref().is_none_or(|t| tags.contains(t));
                    agent_ok && topic_ok
                })
                .take(limit as usize)
                .collect();
            Ok(serde_json::json!({ "notes": notes }).to_string())
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
            let mut counts: std::collections::BTreeMap<String, u32> =
                std::collections::BTreeMap::new();
            for note in db.list(u32::MAX, 0, &Filter::default())?.notes {
                for tag in note.tag_list() {
                    if let Some(topic) = tag.strip_prefix("topic:") {
                        *counts.entry(topic.to_string()).or_insert(0) += 1;
                    }
                }
            }
            let topics: Vec<_> = counts
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
            let tag = format!("topic:{topic}");
            let page = db.search(&tag, max_notes, 0)?;
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

#[tool_handler]
impl ServerHandler for FastxtMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("fastxt-mcp", env!("CARGO_PKG_VERSION")).with_description(
                    "Fastxt local-first notes with on-device AI and agent memory",
                ),
            )
            .with_instructions(
                "Fastxt note store.\n\n\
                 Notes: search_notes, list_notes, get_note, save_note, update_note, delete_note.\n\
                 AI (needs a local backend, see ai_status): suggest_tags, summarize_text, \
                 find_related, get_categories.\n\
                 Agent memory: remember, recall, forget, list_topics, summarize_topic — \
                 memories are notes with agent:/topic:/importance: tags; filter recall by \
                 agent_id or topic.",
            )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // stderr only: stdout carries the MCP protocol.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("starting Fastxt MCP server");
    let server = FastxtMcpServer::new()?;
    let service = server
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await
        .inspect_err(|e| tracing::error!("serving error: {e:?}"))?;
    service.waiting().await?;
    Ok(())
}
