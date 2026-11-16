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

//! Peer-to-peer sync over mutually-authenticated TLS.
//!
//! Protocol v3. The wire contract is independent of the database schema
//! version: a client and a server sync as long as both speak this protocol.
//!
//! - Transport: TLS with a per-session self-signed server certificate. The
//!   client pins the certificate fingerprint from the pairing code; no CA.
//! - Authorization: the pairing token, checked in constant time on every
//!   request. Requests that fail return `None`/`false` rather than errors,
//!   and the client reports "pairing token rejected".
//! - Content: full notes with hybrid-clock stamps and tombstones, merged
//!   per-field-group by whichever side is newer ([`crate::store::Fastxt::apply_remote`]).
//! - Embeddings: exchanged per model, only for notes both sides have at the
//!   same version, so a vector never attaches to text it doesn't describe.
//! - Sent records ([`crate::sent`]): written once, merged by id, after the
//!   notes, so a tombstone has already cleared the records of its note.
//! - Agent sessions (v3): a server given an [`AgentHost`] lists the
//!   coding-agent sessions open on its computer and types a note into the
//!   one a paired device picks. One without answers every such call with a
//!   [`SendRefusal`] that says so.

pub mod client;
pub mod server;

use crate::model::{
    AgentSession, EmbeddingModelInfo, EmbeddingStamp, SendRefusal, Sent, Stamp, SyncEmbedding,
    SyncNote,
};
use tarpc::context;

/// Sync protocol version; bump on any wire-incompatible change.
pub const PROTOCOL_VERSION: u32 = 3;

/// Default sync port.
pub const DEFAULT_PORT: u16 = 3456;

/// Upper bound for one RPC frame (protects the server from memory exhaustion).
pub const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

/// How many records travel in one request.
pub const CHUNK: usize = 100;

/// The sync service. `token` authorizes every call; unauthenticated calls
/// get `None`, which the client turns into a "pairing token rejected" error.
#[tarpc::service]
pub trait FastxtSync {
    /// Protocol version; no auth needed so mismatches surface immediately.
    async fn protocol_version() -> u32;
    /// Is this the token from the pairing code?
    async fn auth(token: String) -> bool;
    /// Version stamps of every note, tombstones included.
    async fn manifest(token: String) -> Option<Vec<Stamp>>;
    /// Full records for the given notes.
    async fn records(token: String, uuids: Vec<String>) -> Option<Vec<SyncNote>>;
    /// Embedding models the server has vectors for.
    async fn embedding_models(token: String) -> Option<Vec<EmbeddingModelInfo>>;
    /// (note, note version) pairs for one embedding model.
    async fn embeddings_manifest(token: String, model_id: String) -> Option<Vec<EmbeddingStamp>>;
    /// Vectors for one model.
    async fn embeddings(
        token: String,
        model_id: String,
        uuids: Vec<String>,
    ) -> Option<Vec<SyncEmbedding>>;
    /// Store notes; returns how many changed.
    async fn send_records(token: String, records: Vec<SyncNote>) -> Option<u32>;
    /// Store embeddings; returns how many were accepted.
    async fn send_embeddings(token: String, records: Vec<SyncEmbedding>) -> Option<u32>;
    /// The id of every sent record.
    async fn sent_manifest(token: String) -> Option<Vec<String>>;
    /// The sent records with these ids.
    async fn sent_records(token: String, uuids: Vec<String>) -> Option<Vec<Sent>>;
    /// Store sent records; returns how many were new.
    async fn send_sent(token: String, records: Vec<Sent>) -> Option<u32>;
    /// The coding-agent sessions open on the server's computer.
    async fn agent_sessions(
        token: String,
    ) -> Option<std::result::Result<Vec<AgentSession>, SendRefusal>>;
    /// Type a note's text into one of those sessions, press Enter, and
    /// record it. The record comes back for the caller to keep too.
    async fn send_to_session(
        token: String,
        note_uuid: String,
        text: String,
        handle: String,
    ) -> Option<std::result::Result<Sent, SendRefusal>>;
}

/// What runs the coding-agent sessions on a server's computer: it lists the
/// ones that are open and types into them. A sync server offers them to
/// paired devices only when it is given one.
///
/// Both calls block, for as long as the program that hosts the sessions
/// takes; the server runs them off its network thread.
pub trait AgentHost: Send + Sync {
    /// The sessions open now.
    ///
    /// # Errors
    /// A [`SendRefusal`] saying why they can't be listed.
    fn sessions(&self) -> std::result::Result<Vec<AgentSession>, SendRefusal>;

    /// Type `text` into the session and press Enter.
    ///
    /// # Errors
    /// A [`SendRefusal`] saying why nothing was typed, or that the text
    /// reached something other than an agent.
    fn send(&self, handle: &str, text: &str) -> std::result::Result<Typed, SendRefusal>;
}

/// What an [`AgentHost`] says about a prompt it typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Typed {
    /// The program that hosts the session.
    pub host: String,
    pub agent: String,
    /// What the session is called.
    pub session: String,
    /// The project folder it works in.
    pub cwd: String,
    /// Whether the agent was seen starting on it.
    pub started: bool,
}

/// A request context with a generous deadline (batches can be large, and a
/// send waits for the agent to start).
pub(crate) fn ctx() -> context::Context {
    let mut context = context::current();
    context.deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    context
}
