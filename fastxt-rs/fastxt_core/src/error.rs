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

//! The error type shared by every `fastxt_core` API.

/// Errors returned by `fastxt_core`. Nothing in the core panics on bad input,
/// a missing database, or a failed network call; it returns one of these.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("database location unavailable: {0}")]
    Path(String),
    #[error("database migration failed: {0}")]
    Migration(String),
    #[error("sync failed: {0}")]
    Sync(String),
    /// The AI backend could not be reached (not running, wrong endpoint).
    #[error("AI backend unavailable: {0}")]
    AiUnavailable(String),
    /// The AI backend was reached but the request failed.
    #[error("AI request failed: {0}")]
    Ai(String),
}

/// `Result` with [`Error`] as the default error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;
