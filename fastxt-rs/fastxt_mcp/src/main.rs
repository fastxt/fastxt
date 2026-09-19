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

use rmcp::{
    ErrorData as McpError, ServerHandler, ServiceExt, handler::server::wrapper::Parameters,
    model::*, schemars, tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// Parameters for searching notes by text query.
#[derive(Debug, Deserialize, JsonSchema)]
struct SearchNotesParams {
    /// Text query to search for in note text and tags
    query: String,
    /// Maximum number of results to return (default: 20)
    limit: Option<u32>,
    /// Number of results to skip (default: 0)
    offset: Option<u32>,
}

/// Parameters for saving a new text note.
#[derive(Debug, Deserialize, JsonSchema)]
struct SaveNoteParams {
    /// The text content of the note
    text: String,
    /// Comma-separated tags for the note (optional, default: empty)
    tags: Option<String>,
}

/// Parameters for listing recent notes.
#[derive(Debug, Deserialize, JsonSchema)]
struct ListNotesParams {
    /// Maximum number of notes to return (default: 20)
    limit: Option<u32>,
    /// Number of notes to skip (default: 0)
    offset: Option<u32>,
}

/// Parameters for deleting a note by its row ID.
#[derive(Debug, Deserialize, JsonSchema)]
struct DeleteNoteParams {
    /// The row ID of the note to delete
    rowid: i64,
}

/// Parameters for AI-tagging a note's text.
#[derive(Debug, Deserialize, JsonSchema)]
struct TagNoteParams {
    /// The row ID of the note to tag
    rowid: i64,
}

/// Parameters for AI-summarizing a note.
#[derive(Debug, Deserialize, JsonSchema)]
struct SummarizeNoteParams {
    /// The row ID of the note to summarize
    rowid: i64,
}

/// Parameters for finding notes related to a given note by embedding similarity.
#[derive(Debug, Deserialize, JsonSchema)]
struct FindRelatedParams {
    /// The row ID of the note to find related notes for
    note_id: i64,
    /// Maximum number of related notes to return (default: 5)
    limit: Option<i64>,
}

// --- Agent memory tool parameters ---

/// Parameters for storing a memory with agent context metadata.
#[derive(Debug, Deserialize, JsonSchema)]
struct RememberParams {
    /// The text content to remember
    text: String,
    /// Comma-separated user tags (optional)
    tags: Option<String>,
    /// Identifier for the agent storing this memory (optional)
    agent_id: Option<String>,
    /// Topic/category for this memory (optional)
    topic: Option<String>,
    /// Importance level: low, medium, or high (optional, default: medium)
    importance: Option<String>,
}

/// Parameters for recalling memories with agent context filtering.
#[derive(Debug, Deserialize, JsonSchema)]
struct RecallParams {
    /// Text query to search for in memories
    query: String,
    /// Filter by agent ID (optional)
    agent_id: Option<String>,
    /// Filter by topic (optional)
    topic: Option<String>,
    /// Maximum number of results to return (default: 10)
    limit: Option<i64>,
}

/// Parameters for deleting a specific memory.
#[derive(Debug, Deserialize, JsonSchema)]
struct ForgetParams {
    /// The row ID of the memory/note to delete
    note_id: i64,
}

/// Parameters for summarizing memories on a topic.
#[derive(Debug, Deserialize, JsonSchema)]
struct SummarizeContextParams {
    /// Topic to summarize memories for
    topic: String,
    /// Maximum number of notes to include in the summary (default: 20)
    max_notes: Option<i64>,
}

/// Parameters for listing all topics (none required).
#[derive(Debug, Deserialize, JsonSchema)]
struct ListTopicsParams {}

/// Run a command through fastxt_core::exe::run and return the JSON result.
fn run_cmd(json: &serde_json::Value) -> String {
    fastxt_core::exe::run(&json.to_string())
}

/// Build a combined tags string from user tags and agent metadata key:value pairs.
fn build_agent_tags(
    user_tags: Option<&str>,
    agent_id: Option<&str>,
    topic: Option<&str>,
    importance: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(aid) = agent_id
        && !aid.is_empty()
    {
        parts.push(format!("agent:{aid}"));
    }
    if let Some(t) = topic
        && !t.is_empty()
    {
        parts.push(format!("topic:{t}"));
    }
    let imp = importance.unwrap_or("medium");
    parts.push(format!("importance:{imp}"));
    if let Some(ut) = user_tags
        && !ut.is_empty()
    {
        parts.push(ut.to_string());
    }
    parts.join(",")
}

/// The Fastxt MCP server exposing note operations as tools.
#[derive(Clone)]
struct FastxtMcpServer;

#[tool_router]
impl FastxtMcpServer {
    fn new() -> Self {
        // Initialize the database on server startup
        let conn = fastxt_core::exe::get_sqlite_connection();
        fastxt_core::exe::ensure_db_initialized(&conn);

        Self
    }

    #[tool(
        name = "search_notes",
        description = "Search notes by text query. Searches in both note text and tags."
    )]
    async fn search_notes(
        &self,
        Parameters(params): Parameters<SearchNotesParams>,
    ) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "search",
            "query": params.query,
            "limit": params.limit.unwrap_or(20),
            "offset": params.offset.unwrap_or(0),
        }));
        Ok(result)
    }

    #[tool(
        name = "save_note",
        description = "Save a new text note with optional tags. Returns the updated list of recent notes."
    )]
    async fn save_note(
        &self,
        Parameters(params): Parameters<SaveNoteParams>,
    ) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "insert",
            "txt": params.text,
            "tags": params.tags.unwrap_or_default(),
            "limit": 10,
            "offset": 0,
        }));
        Ok(result)
    }

    #[tool(
        name = "list_notes",
        description = "List recent notes ordered by creation date (newest first)."
    )]
    async fn list_notes(
        &self,
        Parameters(params): Parameters<ListNotesParams>,
    ) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "select",
            "limit": params.limit.unwrap_or(20),
            "offset": params.offset.unwrap_or(0),
        }));
        Ok(result)
    }

    #[tool(
        name = "delete_note",
        description = "Delete a note by its row ID. Returns the updated search results."
    )]
    async fn delete_note(
        &self,
        Parameters(params): Parameters<DeleteNoteParams>,
    ) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "delete",
            "rowid": params.rowid,
            "query": "",
            "limit": 20,
            "offset": 0,
        }));
        Ok(result)
    }

    #[tool(
        name = "tag_note",
        description = "Generate AI tags for a note by its row ID. Requires an AI backend (e.g., Ollama) to be running."
    )]
    async fn tag_note(
        &self,
        Parameters(params): Parameters<TagNoteParams>,
    ) -> Result<String, McpError> {
        // Get the note text by querying the database directly
        let conn = fastxt_core::exe::get_sqlite_connection();
        let txt: Option<String> = conn
            .query_row(
                "SELECT txt FROM note WHERE rowid = ?1",
                [&params.rowid],
                |row| row.get(0),
            )
            .ok();

        match txt {
            Some(text) => {
                let result = run_cmd(&serde_json::json!({
                    "action": "ai-tag",
                    "text": text,
                }));
                Ok(result)
            }
            None => Ok(format!(
                r#"{{"error":"Note with rowid {} not found"}}"#,
                params.rowid
            )),
        }
    }

    #[tool(
        name = "summarize_note",
        description = "Generate an AI summary for a note by its row ID. Requires an AI backend (e.g., Ollama) to be running."
    )]
    async fn summarize_note(
        &self,
        Parameters(params): Parameters<SummarizeNoteParams>,
    ) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "ai-summarize",
            "rowid": params.rowid,
        }));
        Ok(result)
    }

    #[tool(
        name = "get_categories",
        description = "List all AI-assigned categories with their note counts."
    )]
    async fn get_categories(&self) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "get-categories",
        }));
        Ok(result)
    }

    #[tool(
        name = "find_related",
        description = "Find notes related to a given note by embedding similarity. Requires embeddings to have been generated for notes (via ai-embed or ai-embed-all)."
    )]
    async fn find_related(
        &self,
        Parameters(params): Parameters<FindRelatedParams>,
    ) -> Result<String, McpError> {
        let result = run_cmd(&serde_json::json!({
            "action": "related",
            "rowid": params.note_id,
            "limit": params.limit.unwrap_or(5),
        }));
        Ok(result)
    }

    // --- Agent memory tools ---

    #[tool(
        name = "remember",
        description = "Store a memory with agent context metadata. Saves text as a note with structured tags encoding agent_id, topic, and importance level. Use this to persist facts, decisions, or context that should be recalled later."
    )]
    async fn remember(
        &self,
        Parameters(params): Parameters<RememberParams>,
    ) -> Result<String, McpError> {
        let tags = build_agent_tags(
            params.tags.as_deref(),
            params.agent_id.as_deref(),
            params.topic.as_deref(),
            params.importance.as_deref(),
        );
        // Insert the note
        run_cmd(&serde_json::json!({
            "action": "insert",
            "txt": params.text,
            "tags": tags,
            "limit": 1,
            "offset": 0,
        }));
        // Query the most recently created note to return its rowid
        let conn = fastxt_core::exe::get_sqlite_connection();
        let rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM note ORDER BY rowid DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);
        Ok(serde_json::json!({ "rowid": rowid, "status": "remembered" }).to_string())
    }

    #[tool(
        name = "recall",
        description = "Search memories with optional agent context filtering. Searches note text and tags, then filters results by agent_id and/or topic if provided. Returns matching memories ordered by relevance."
    )]
    async fn recall(
        &self,
        Parameters(params): Parameters<RecallParams>,
    ) -> Result<String, McpError> {
        let limit = params.limit.unwrap_or(10);
        // Search with a generous limit so we can filter down
        let fetch_limit = limit * 5;
        let result = run_cmd(&serde_json::json!({
            "action": "search",
            "query": params.query,
            "limit": fetch_limit,
            "offset": 0,
        }));

        // Parse, filter by agent_id/topic tags, and re-limit
        let mut parsed: serde_json::Value =
            serde_json::from_str(&result).unwrap_or(serde_json::json!({"notes": []}));

        if let Some(notes) = parsed.get_mut("notes").and_then(|n| n.as_array_mut()) {
            let agent_filter = params.agent_id.as_deref().map(|a| format!("agent:{a}"));
            let topic_filter = params.topic.as_deref().map(|t| format!("topic:{t}"));

            notes.retain(|note| {
                let tags = note.get("tags").and_then(|t| t.as_str()).unwrap_or("");
                let agent_ok = agent_filter
                    .as_ref()
                    .is_none_or(|af| tags.contains(af.as_str()));
                let topic_ok = topic_filter
                    .as_ref()
                    .is_none_or(|tf| tags.contains(tf.as_str()));
                agent_ok && topic_ok
            });
            notes.truncate(limit as usize);
        }

        Ok(parsed.to_string())
    }

    #[tool(
        name = "forget",
        description = "Delete a specific memory by its note ID. Permanently removes the memory from the database."
    )]
    async fn forget(
        &self,
        Parameters(params): Parameters<ForgetParams>,
    ) -> Result<String, McpError> {
        run_cmd(&serde_json::json!({
            "action": "delete",
            "rowid": params.note_id,
            "query": "",
            "limit": 1,
            "offset": 0,
        }));
        Ok(serde_json::json!({ "status": "forgotten", "note_id": params.note_id }).to_string())
    }

    #[tool(
        name = "summarize_context",
        description = "Get an AI-generated summary of memories on a given topic. Searches for notes tagged with the specified topic, concatenates their text, and produces a summary. Requires an AI backend (e.g., Ollama) to be running."
    )]
    async fn summarize_context(
        &self,
        Parameters(params): Parameters<SummarizeContextParams>,
    ) -> Result<String, McpError> {
        let max_notes = params.max_notes.unwrap_or(20);
        let topic_tag = format!("topic:{}", params.topic);
        // Search for notes with this topic tag
        let result = run_cmd(&serde_json::json!({
            "action": "search",
            "query": topic_tag,
            "limit": max_notes,
            "offset": 0,
        }));

        let parsed: serde_json::Value =
            serde_json::from_str(&result).unwrap_or(serde_json::json!({"notes": []}));

        let notes = parsed
            .get("notes")
            .and_then(|n| n.as_array())
            .cloned()
            .unwrap_or_default();

        if notes.is_empty() {
            return Ok(serde_json::json!({
                "summary": null,
                "note_count": 0,
                "topic": params.topic,
                "error": "No memories found for this topic"
            })
            .to_string());
        }

        // Concatenate all note texts
        let combined: String = notes
            .iter()
            .filter_map(|n| n.get("txt").and_then(|t| t.as_str()))
            .collect::<Vec<&str>>()
            .join("\n\n---\n\n");

        // Insert a temporary note with combined text, summarize it, then clean up
        let conn = fastxt_core::exe::get_sqlite_connection();
        run_cmd(&serde_json::json!({
            "action": "insert",
            "txt": combined,
            "tags": format!("_temp_summary,topic:{}", params.topic),
            "limit": 1,
            "offset": 0,
        }));

        // Get the rowid of the just-inserted temp note
        let temp_rowid: i64 = conn
            .query_row(
                "SELECT rowid FROM note ORDER BY rowid DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(-1);

        if temp_rowid < 0 {
            return Ok(
                serde_json::json!({"error": "Failed to create temporary note for summarization"})
                    .to_string(),
            );
        }

        // Summarize the temporary note
        let summary_result = run_cmd(&serde_json::json!({
            "action": "ai-summarize",
            "rowid": temp_rowid,
        }));

        // Delete the temporary note
        run_cmd(&serde_json::json!({
            "action": "delete",
            "rowid": temp_rowid,
            "query": "",
            "limit": 1,
            "offset": 0,
        }));

        // Extract the summary from the response
        let summary_parsed: serde_json::Value =
            serde_json::from_str(&summary_result).unwrap_or(serde_json::json!({}));
        let summary = summary_parsed
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("");

        Ok(serde_json::json!({
            "summary": summary,
            "note_count": notes.len(),
            "topic": params.topic,
        })
        .to_string())
    }

    #[tool(
        name = "list_topics",
        description = "List all topics stored by agents with their memory counts. Scans all notes for topic:* tags and returns unique topic names."
    )]
    async fn list_topics(
        &self,
        Parameters(_params): Parameters<ListTopicsParams>,
    ) -> Result<String, McpError> {
        let conn = fastxt_core::exe::get_sqlite_connection();
        let mut stmt = conn
            .prepare("SELECT tags FROM note WHERE tags LIKE '%topic:%'")
            .map_err(|e| McpError {
                code: ErrorCode::INTERNAL_ERROR,
                message: format!("Database query failed: {e}").into(),
                data: None,
            })?;

        let mut topic_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();

        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| McpError {
                code: ErrorCode::INTERNAL_ERROR,
                message: format!("Query failed: {e}").into(),
                data: None,
            })?;

        for row in rows.flatten() {
            for tag in row.split(',') {
                let tag = tag.trim();
                if let Some(topic) = tag.strip_prefix("topic:")
                    && !topic.is_empty()
                {
                    *topic_counts.entry(topic.to_string()).or_insert(0) += 1;
                }
            }
        }

        // Convert to a sorted list of {topic, count} objects
        let mut topics: Vec<serde_json::Value> = topic_counts
            .into_iter()
            .map(|(topic, count)| serde_json::json!({"topic": topic, "count": count}))
            .collect();
        topics.sort_by(|a, b| {
            b.get("count")
                .and_then(|c| c.as_u64())
                .unwrap_or(0)
                .cmp(&a.get("count").and_then(|c| c.as_u64()).unwrap_or(0))
        });

        Ok(serde_json::json!({ "topics": topics }).to_string())
    }
}

#[tool_handler]
impl ServerHandler for FastxtMcpServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("fastxt-mcp", env!("CARGO_PKG_VERSION")).with_description(
                    "MCP server for Fastxt local-first text notes with agent memory capabilities",
                ),
            )
            .with_instructions(
                "Fastxt MCP server for managing local text notes and agent memory.\n\n\
                 ## Note management tools:\n\
                 - search_notes: Search notes by text query\n\
                 - save_note: Save a new text note\n\
                 - list_notes: List recent notes\n\
                 - delete_note: Delete a note by ID\n\
                 - tag_note: AI-tag a note (requires Ollama)\n\
                 - summarize_note: AI-summarize a note (requires Ollama)\n\
                 - get_categories: List all AI categories\n\
                 - find_related: Find related notes by embedding similarity\n\n\
                 ## Agent memory tools:\n\
                 Use remember/recall/forget for persistent agent memory.\n\
                 Memories are notes with structured metadata tags (agent_id, topic, importance).\n\
                 - remember: Store a memory with agent context (agent_id, topic, importance)\n\
                 - recall: Search memories with optional agent/topic filtering\n\
                 - forget: Delete a specific memory by ID\n\
                 - summarize_context: AI-summarize all memories on a topic\n\
                 - list_topics: List all topics with memory counts\n\n\
                 Typical workflow: remember facts/decisions -> recall relevant context -> \
                 summarize_context for overviews -> forget outdated memories.",
            )
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Log to stderr so stdout is reserved for MCP JSON-RPC
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting Fastxt MCP Server");

    let server = FastxtMcpServer::new();
    let service = server
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
