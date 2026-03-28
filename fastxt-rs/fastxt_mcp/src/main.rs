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
    ErrorData as McpError, ServerHandler, ServiceExt, handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, schemars, tool, tool_handler, tool_router,
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

/// Run a command through fastxt_core::exe::run and return the JSON result.
fn run_cmd(json: &serde_json::Value) -> String {
    fastxt_core::exe::run(&json.to_string())
}

/// The Fastxt MCP server exposing note operations as tools.
#[derive(Clone)]
struct FastxtMcpServer {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl FastxtMcpServer {
    fn new() -> Self {
        // Initialize the database on server startup
        let conn = fastxt_core::exe::get_sqlite_connection();
        fastxt_core::exe::ensure_db_initialized(&conn);

        Self {
            tool_router: Self::tool_router(),
        }
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
}

#[tool_handler]
impl ServerHandler for FastxtMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "fastxt-mcp".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: Some("MCP server for Fastxt local-first text notes".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Fastxt MCP server for managing local text notes.\n\n\
                 Available tools:\n\
                 - search_notes: Search notes by text query\n\
                 - save_note: Save a new text note\n\
                 - list_notes: List recent notes\n\
                 - delete_note: Delete a note by ID\n\
                 - tag_note: AI-tag a note (requires Ollama)\n\
                 - summarize_note: AI-summarize a note (requires Ollama)\n\
                 - get_categories: List all AI categories"
                    .to_string(),
            ),
            ..Default::default()
        }
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
