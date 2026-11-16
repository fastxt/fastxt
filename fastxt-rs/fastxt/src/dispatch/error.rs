/*
    Fastxt
    Copyright (C) 2026  Yi Wang

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

//! The error type of the dispatcher.

/// Errors returned by the dispatcher. A missing program, a refused send
/// or output that can't be read comes back as one of these; nothing panics.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The command of the program that hosts the sessions isn't there to run.
    #[error("{0} is not installed, or its command was not found")]
    NotInstalled(String),
    #[error("no open session matches {0}")]
    NoSession(String),
    #[error("{wanted} matches {count} open sessions; give more of it")]
    Ambiguous { wanted: String, count: usize },
    /// The session's window closed, or its agent ended, since it was listed.
    #[error("session {0} is no longer open")]
    Gone(String),
    /// The agent is showing a permission or trust question. A prompt typed
    /// there, with Enter after it, would answer the question, so nothing was
    /// typed.
    #[error("session {0} is asking a question; answer it there, then send again")]
    Asking(String),
    /// The prompt was typed, and no agent was there to take it: it went to
    /// the shell of that terminal, which ran it as a command. The agent quit
    /// in the instant between the check and the send. Look at that terminal.
    #[error(
        "the agent in session {0} had just quit: the prompt was typed into the shell of that terminal instead. Look at it now"
    )]
    NotAnAgent(String),
    /// The host's command ran and said no.
    #[error("{program} failed: {message}")]
    Failed { program: String, message: String },
    #[error("{program} did not finish within {seconds} seconds")]
    Timeout { program: String, seconds: u64 },
    /// The command succeeded but printed something this version can't read.
    #[error("could not read what {program} printed: {message}")]
    Unreadable { program: String, message: String },
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// `Result` with [`Error`] as the default error type.
pub type Result<T, E = Error> = std::result::Result<T, E>;
