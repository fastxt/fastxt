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
use crate::Note;
#[tarpc::service]
pub trait Fastxt {
    async fn is_version_match(version: String) -> bool;
    async fn diff_uuid4_to_server(candidates: Vec<String>) -> Vec<String>;
    async fn diff_uuid4_from_server(candidates: Vec<String>) -> Vec<String>;
    async fn send_note(note: Note) -> bool;
    async fn receive_note(uuid4: String) -> Note;
    async fn stop() -> bool;
    // Embedding sync methods
    /// Get the embedding model ID used by this device
    async fn get_embedding_model_id() -> Option<String>;
    /// Get note UUIDs that have embeddings with a specific model ID
    async fn get_embedding_uuid4s(model_id: String) -> Vec<String>;
    /// Receive embedding data for a note (uuid4, embedding bytes, model_id)
    async fn receive_embedding(uuid4: String) -> Option<(Vec<u8>, String)>;
    /// Send embedding data to be stored on the server
    async fn send_embedding(uuid4: String, embedding_bytes: Vec<u8>, model_id: String) -> bool;
}

pub mod client;
pub mod server;
