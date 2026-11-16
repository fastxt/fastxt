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

//! Data types exchanged with every `fastxt_core` caller.

use serde::{Deserialize, Serialize};

/// A saved note.
///
/// `rowid` is local to one database; `uuid4` identifies the note on every
/// device. `updated_at` / `ai_updated_at` are hybrid-logical-clock stamps
/// (see [`crate::clock`]) used to merge concurrent changes during sync.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Note {
    #[serde(default)]
    pub rowid: i64,
    #[serde(default)]
    pub uuid4: String,
    #[serde(default)]
    pub txt: String,
    /// Comma-separated user tags (normalised; tags may contain spaces).
    #[serde(default)]
    pub tags: String,
    /// UTC creation time, `%Y-%m-%d %H:%M:%S`.
    #[serde(default)]
    pub created_at: String,
    /// AI-suggested tags as a JSON array string, e.g. `["rust","sync"]`.
    #[serde(default)]
    pub ai_tags: Option<String>,
    #[serde(default)]
    pub ai_summary: Option<String>,
    #[serde(default)]
    pub ai_category: Option<String>,
    /// Clock stamp of the last change to `txt` or `tags`.
    #[serde(default)]
    pub updated_at: String,
    /// Clock stamp of the last change to the AI fields (empty if none yet).
    #[serde(default)]
    pub ai_updated_at: String,
}

impl Note {
    /// The AI tags as a list (empty when there are none).
    #[must_use]
    pub fn ai_tag_list(&self) -> Vec<String> {
        self.ai_tags
            .as_deref()
            .map(crate::tags::parse_ai_tags)
            .unwrap_or_default()
    }

    /// The user tags as a list.
    #[must_use]
    pub fn tag_list(&self) -> Vec<String> {
        crate::tags::split_tags(&self.tags)
    }
}

/// Identifies a note by its local rowid or its global UUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteKey {
    Rowid(i64),
    Uuid(String),
}

impl From<i64> for NoteKey {
    fn from(rowid: i64) -> Self {
        NoteKey::Rowid(rowid)
    }
}

impl From<&str> for NoteKey {
    fn from(uuid: &str) -> Self {
        NoteKey::Uuid(uuid.to_string())
    }
}

impl From<String> for NoteKey {
    fn from(uuid: String) -> Self {
        NoteKey::Uuid(uuid)
    }
}

impl std::fmt::Display for NoteKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoteKey::Rowid(r) => write!(f, "rowid {r}"),
            NoteKey::Uuid(u) => write!(f, "uuid {u}"),
        }
    }
}

/// Fields for a new note. AI fields are optional so a client that generated
/// a summary or tags before saving can store them with the note.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewNote {
    pub txt: String,
    pub tags: String,
    pub ai_tags: Option<Vec<String>>,
    pub ai_summary: Option<String>,
    pub ai_category: Option<String>,
}

impl NewNote {
    #[must_use]
    pub fn new(txt: impl Into<String>, tags: impl Into<String>) -> Self {
        NewNote {
            txt: txt.into(),
            tags: tags.into(),
            ..Default::default()
        }
    }
}

/// One page of notes plus the total number matching the query.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub count: u32,
    pub notes: Vec<Note>,
}

/// A note with a relevance score (similarity or fused rank score).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ScoredNote {
    pub note: Note,
    pub score: f64,
}

/// An AI category with the number of notes in it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct CategoryCount {
    pub category: String,
    pub count: u32,
}

/// A tag with the number of notes using it (user and AI tags combined).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub tag: String,
    pub count: u32,
}

/// An embedding model present in the database.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingModelInfo {
    pub model_id: String,
    pub dim: usize,
    pub count: u32,
}

/// Optional filters for listing and searching notes; conditions combine
/// with AND.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Only notes with this AI category.
    pub category: Option<String>,
    /// Only notes carrying every one of these user tags. Whole tags only
    /// (`go` does not match `golang`), ASCII case-insensitive.
    pub tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// Sync wire types
// ---------------------------------------------------------------------------

/// A note as exchanged during sync, including tombstones.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct SyncNote {
    pub uuid4: String,
    pub txt: String,
    pub tags: String,
    pub created_at: String,
    pub ai_tags: Option<String>,
    pub ai_summary: Option<String>,
    pub ai_category: Option<String>,
    pub updated_at: String,
    pub ai_updated_at: String,
    pub deleted: bool,
}

/// The version stamps of one note, used to decide what to transfer.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub uuid4: String,
    pub updated_at: String,
    pub ai_updated_at: String,
}

/// An embedding as exchanged during sync. `note_updated_at` is the note
/// version the vector was computed from; receivers only accept vectors that
/// match their own copy of the note.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SyncEmbedding {
    pub note_uuid: String,
    pub model_id: String,
    pub note_updated_at: String,
    pub vector: Vec<f32>,
}

/// The version of the note an embedding was computed from.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingStamp {
    pub note_uuid: String,
    pub note_updated_at: String,
}

// ---------------------------------------------------------------------------
// Sending a note to a coding-agent session
// ---------------------------------------------------------------------------

/// A coding-agent session open in a window on the computer a server runs on:
/// one its user started and can see.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentSession {
    /// The program that hosts its window, e.g. `orca`.
    pub host: String,
    /// The host's handle for that window. It lasts until the host or the
    /// window restarts, so ask for the list again rather than keep one.
    pub handle: String,
    /// The agent, as the host names it: `claude`, `codex`, `kimi`, …
    pub agent: String,
    /// What the session is called there.
    pub name: String,
    /// The project folder it works in.
    pub cwd: String,
    pub branch: Option<String>,
    /// `ready`, `working`, `asking`, or empty when the host doesn't say.
    pub state: String,
    /// The start of the last prompt it was given, to tell sessions apart.
    pub last_prompt: Option<String>,
    /// When it last changed, in milliseconds since 1970.
    pub updated_at: Option<u64>,
}

/// Why a note wasn't typed into a session, or the sessions couldn't be
/// listed. `reason` is for programs, `message` for people.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SendRefusal {
    /// One of the `SendRefusal::*` words.
    pub reason: String,
    pub message: String,
}

impl SendRefusal {
    /// This server doesn't send to sessions, or the program that hosts them
    /// isn't installed.
    pub const UNAVAILABLE: &'static str = "unavailable";
    /// The session closed since it was listed.
    pub const GONE: &'static str = "gone";
    /// The session is asking its user a question; nothing was typed.
    pub const ASKING: &'static str = "asking";
    /// The text went to a shell, not an agent. Someone should look.
    pub const NOT_AN_AGENT: &'static str = "not-an-agent";
    /// The request can't be carried out as given (empty text, bad handle).
    pub const INVALID: &'static str = "invalid";
    /// Anything else, in the host's words.
    pub const FAILED: &'static str = "failed";

    #[must_use]
    pub fn new(reason: &str, message: impl Into<String>) -> Self {
        SendRefusal {
            reason: reason.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SendRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// A record that a note was typed into a session: where and when it went.
/// Written once and never changed; it goes when its note is deleted.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct Sent {
    /// The record's own id.
    pub uuid4: String,
    /// The note that was sent.
    pub note_uuid: String,
    /// UTC, `%Y-%m-%d %H:%M:%S`.
    pub sent_at: String,
    /// The program that hosted the session.
    pub host: String,
    pub agent: String,
    /// What the session was called.
    pub session: String,
    /// The project folder the session worked in.
    pub cwd: String,
    /// Whether the agent was seen starting on it.
    pub started: bool,
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Per-database settings, stored in the `meta` table so every client (desktop,
/// MCP server, CLI, mobile) sees the same configuration.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub ai: AiSettings,
}

/// AI backend configuration.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct AiSettings {
    /// `ollama`, `llamacpp`, `foundry-local` or `openai` (any OpenAI-compatible server).
    pub backend: String,
    /// Base URL; empty means the backend's default local address.
    pub endpoint: String,
    /// Model used for tagging, summaries and categories.
    pub model: String,
    /// Model used for embeddings (semantic search). Kept separate from
    /// `model` because chat models make poor embedders and vice versa.
    pub embedding_model: String,
    pub timeout_secs: u64,
    /// Self-consistency voting for tags: run tagging this many times and keep
    /// tags that win a majority. 1 disables voting.
    pub consistency_rounds: u32,
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            backend: "ollama".to_string(),
            endpoint: String::new(),
            model: "llama3.2".to_string(),
            embedding_model: "nomic-embed-text".to_string(),
            timeout_secs: 60,
            consistency_rounds: 1,
        }
    }
}
