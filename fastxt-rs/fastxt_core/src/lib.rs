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

//! `fastxt_core` — shared library for Fastxt.
//!
//! Provides the `SQLite` database layer, JSON command dispatcher ([`exe::run`]),
//! P2P RPC sync protocol ([`rpc`]), database upgrade logic ([`upgrade`]), and
//! optional on-device AI features ([`ai`], enabled with `--features ai`).
//!
//! The FFI entry points ([`fastxt_run`] / [`fastxt_free`]) expose the command
//! dispatcher to mobile platforms via a C-compatible interface.

use serde_derive::{Deserialize, Serialize};

pub mod cmd;
pub mod exe;
pub mod rpc;
pub mod upgrade;

#[cfg(feature = "ai")]
pub mod ai;

/// Top-level command envelope sent to [`exe::run`].
/// The `action` field selects which command to execute.
#[derive(Serialize, Deserialize, Debug)]
pub struct Cmd {
    pub action: String,
}

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// FFI entry point: execute a JSON command and return a JSON response.
///
/// # Safety
/// `json_input` must be a valid, non-null, null-terminated C string for the
/// duration of this call. The returned pointer is heap-allocated and **must**
/// be freed by calling [`fastxt_free`].
///
/// # Panics
/// Panics if the hardcoded fallback error JSON string contains an internal null byte
/// (which should be impossible in practice).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastxt_run(json_input: *const c_char) -> *mut c_char {
    let c_str = unsafe { CStr::from_ptr(json_input) };
    let json = match c_str.to_str() {
        Err(_) => r#"{"error": "ios json input error"}"#.to_string(),
        Ok(text) => exe::run(text),
    };

    CString::new(json)
        .or_else(|_| CString::new(r#"{"error":"response contains null byte"}"#))
        .expect("static error string should never contain null bytes")
        .into_raw()
}

/// FFI entry point: free a string previously returned by [`fastxt_run`].
///
/// # Safety
/// `s` must be a pointer originally returned by `fastxt_run`, or null.
/// Passing any other pointer is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fastxt_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    };
}

/// A generic key/value pair with a string key and 64-bit integer value.
#[derive(Serialize, Deserialize, Debug)]
pub struct KVStringI64 {
    pub k: String,
    pub v: i64,
}

/// Wrapper for a comma-separated tags string.
#[derive(Serialize, Deserialize, Debug)]
pub struct Tags {
    pub tags: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Note {
    #[serde(default)]
    pub rowid: i64,
    #[serde(default)]
    pub uuid4: String,
    #[serde(default)]
    pub txt: String,
    #[serde(default)]
    pub tags: String,
    #[serde(default)]
    pub created_at: String,
    /// AI-suggested tags (JSON array string)
    #[serde(default)]
    pub ai_tags: Option<String>,
    /// AI-generated summary
    #[serde(default)]
    pub ai_summary: Option<String>,
    /// AI-assigned category
    #[serde(default)]
    pub ai_category: Option<String>,
}

/// Parameters for the `select` command (paginated listing of all notes).
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdSelect {
    pub limit: u32,
    pub offset: u32,
}

/// Parameters for the `insert` command.
/// After inserting, returns a paginated list of notes at the given offset.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdInsert {
    pub txt: String,
    pub tags: String,

    pub limit: u32,
    pub offset: u32,
}

/// Parameters for the `delete` command.
/// After deleting, re-runs the given search query and returns results.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdDelete {
    pub query: String,
    pub rowid: i64,

    pub limit: u32,
    pub offset: u32,
}

/// Parameters for the `search` command (keyword search over txt and tags).
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdSearch {
    pub query: String,

    pub limit: u32,
    pub offset: u32,
}

/// Parameters for RPC client commands (`client-sync`, `client-stop-server`, `sync-embeddings`).
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdRpcClient {
    pub addr: String,
}

/// Parameters for the `server` command (start the RPC sync server).
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdRpcServer {
    pub addr: String,
}

/// Wrapper used internally to deserialize a single string column from `SQLite`.
#[derive(Serialize, Deserialize, Debug)]
pub struct OneString {
    pub s: String,
}

// AI command structs

#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiTag {
    /// Text to analyze for tag suggestions
    pub text: String,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name (e.g., "llama3.2")
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiTagAll {
    /// Maximum number of notes to process
    pub limit: Option<u32>,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiSummarize {
    /// Rowid of the note to summarize
    pub rowid: i64,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AiTagsResponse {
    /// Suggested tags from AI
    pub tags: Vec<String>,
    /// Whether AI backend was available
    pub available: bool,
    /// Error message if any
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AiSummarizeResponse {
    /// Generated summary
    pub summary: Option<String>,
    /// Whether AI backend was available
    pub available: bool,
    /// Error message if any
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CmdSemanticSearch {
    /// Query text to search for
    pub query: String,
    /// Maximum number of results
    pub limit: Option<u32>,
    /// Minimum similarity threshold (0.0 - 1.0)
    pub threshold: Option<f32>,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name for embeddings
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiEmbed {
    /// Rowid of the note to embed
    pub rowid: i64,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name for embeddings
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiEmbedAll {
    /// Maximum number of notes to process
    pub limit: Option<u32>,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name for embeddings
    pub model: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SemanticSearchResult {
    pub note: Note,
    pub similarity: f32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SemanticSearchResponse {
    pub results: Vec<SemanticSearchResult>,
    pub available: bool,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AiEmbedResponse {
    pub success: bool,
    pub available: bool,
    pub error: Option<String>,
}

/// Command to reprocess AI metadata using local device's model.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiReprocess {
    /// Optional: Rowid to reprocess (if None, reprocess all)
    pub rowid: Option<i64>,
}

/// Command to organize notes by AI-generated categories.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiOrganize {
    /// Maximum number of notes to process
    pub limit: Option<u32>,
    /// Optional: Ollama endpoint URL
    pub endpoint: Option<String>,
    /// Optional: Model name
    pub model: Option<String>,
}

/// Response from ai-organize command.
#[derive(Serialize, Deserialize, Debug)]
pub struct AiOrganizeResponse {
    /// Number of notes processed
    pub processed: u32,
    /// Number of categorization errors
    pub errors: u32,
    /// Category distribution (category -> count)
    pub categories: std::collections::HashMap<String, u32>,
    /// Whether AI backend was available
    pub available: bool,
    /// Error message if any
    pub error: Option<String>,
}

/// Command to simplify text to plain, accessible language.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiSimplify {
    pub text: String,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub backend: Option<String>,
}

/// Response from ai-simplify command.
#[derive(Serialize, Deserialize, Debug)]
pub struct AiSimplifyResponse {
    pub simplified: Option<String>,
    pub available: bool,
    pub error: Option<String>,
}

/// Command to extract key points from text.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiKeyPoints {
    pub text: String,
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub backend: Option<String>,
}

/// Response from ai-key-points command.
#[derive(Serialize, Deserialize, Debug)]
pub struct AiKeyPointsResponse {
    pub key_points: Vec<String>,
    pub available: bool,
    pub error: Option<String>,
}

/// Command to check or select an AI backend.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAiBackend {
    pub backend: String,
    pub endpoint: Option<String>,
}

/// Response from ai-backend command.
#[derive(Serialize, Deserialize, Debug)]
pub struct AiBackendResponse {
    pub backend: String,
    pub available: bool,
    pub error: Option<String>,
}

/// Command to rename a category.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdRenameCategory {
    /// Current category name
    pub old_name: String,
    /// New category name
    pub new_name: String,
}

/// Response from rename-category command.
#[derive(Serialize, Deserialize, Debug)]
pub struct RenameCategoryResponse {
    /// Number of notes updated
    pub updated: usize,
}

/// Command to dismiss (clear) a category.
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdDismissCategory {
    /// Category to dismiss
    pub category: String,
}

/// Response from dismiss-category command.
#[derive(Serialize, Deserialize, Debug)]
pub struct DismissCategoryResponse {
    /// Number of notes updated
    pub updated: usize,
}
