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

//! `fastxt_core` — local-first text notes with on-device AI and P2P sync.
//!
//! - [`store::Fastxt`] is the typed API every client should use: notes,
//!   tags, categories, settings, sync merge.
//! - [`search`] adds full-text (trigram FTS5) and hybrid (RRF) search;
//!   [`vector`] adds semantic search over embeddings from any model.
//! - [`ai`] tags, summarizes and categorizes through a local backend
//!   (Ollama or any OpenAI-compatible server such as llama.cpp).
//! - [`sync`] syncs devices over TLS with QR-code pairing: fingerprints
//!   pin the certificate, a token authorizes every request.
//! - [`json`] + [`ffi`] expose the same operations to the mobile apps
//!   as a JSON-over-C-ABI protocol.
//!
//! Nothing in this crate panics on bad input, a missing database or a dead
//! network: errors come back as [`error::Error`].

pub mod ai;
pub mod clock;
pub mod error;
pub mod ffi;
pub mod json;
pub mod model;
pub mod pairing;
pub mod schema;
pub mod search;
pub mod store;
pub mod sync;
pub mod tags;
pub mod vector;

pub use error::{Error, Result};
pub use model::{Note, NoteKey, Page, Settings};
pub use store::{Fastxt, SharedDb, default_db_path};
