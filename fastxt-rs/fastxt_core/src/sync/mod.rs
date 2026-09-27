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
//! Protocol v2. The wire contract is independent of the database schema
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

pub mod client;
pub mod server;

use crate::model::{EmbeddingModelInfo, EmbeddingStamp, Stamp, SyncEmbedding, SyncNote};
use tarpc::context;

/// Sync protocol version; bump on any wire-incompatible change.
pub const PROTOCOL_VERSION: u32 = 2;

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
}

/// A request context with a generous deadline (batches can be large).
pub(crate) fn ctx() -> context::Context {
    let mut context = context::current();
    context.deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    context
}
