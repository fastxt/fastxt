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

//! Data types of the dispatcher: sessions, receipts and what it runs.

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// A program to run and the arguments that always come first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl Tool {
    /// A program found on the `PATH` by this name.
    #[must_use]
    pub fn named(name: &str) -> Self {
        Tool {
            program: PathBuf::from(name),
            args: Vec::new(),
        }
    }

    /// The program's name, for messages.
    #[must_use]
    pub fn name(&self) -> String {
        self.program.file_name().map_or_else(
            || self.program.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// Orca's command. Orca doesn't put it on the `PATH` by itself, so it is
    /// looked for where Orca says it is (`ORCA_CLI_BIN_DIR`, set in its own
    /// terminals), then on the `PATH`, then inside the app where macOS keeps
    /// it. Found nowhere, it is `orca`, and running it says it isn't
    /// installed.
    #[must_use]
    pub fn orca() -> Self {
        let bin_dir = std::env::var_os("ORCA_CLI_BIN_DIR").map(PathBuf::from);
        let path = std::env::var_os("PATH");
        find_orca(
            bin_dir.as_deref(),
            path.as_deref(),
            dirs::home_dir().as_deref(),
            &|candidate| candidate.is_file(),
        )
    }
}

const ORCA_IN_APP: &str = "Applications/Orca.app/Contents/Resources/bin";

fn find_orca(
    bin_dir: Option<&Path>,
    path: Option<&OsStr>,
    home: Option<&Path>,
    is_file: &dyn Fn(&Path) -> bool,
) -> Tool {
    let name = if cfg!(windows) { "orca.exe" } else { "orca" };
    let mut folders: Vec<PathBuf> = Vec::new();
    folders.extend(bin_dir.map(Path::to_path_buf));
    folders.extend(path.into_iter().flat_map(std::env::split_paths));
    folders.push(Path::new("/").join(ORCA_IN_APP));
    folders.extend(home.map(|home| home.join(ORCA_IN_APP)));
    folders
        .into_iter()
        .filter(|folder| !folder.as_os_str().is_empty())
        .map(|folder| folder.join(name))
        .find(|candidate| is_file(candidate))
        .map_or_else(
            || Tool::named("orca"),
            |program| Tool {
                program,
                args: Vec::new(),
            },
        )
}

/// The commands the dispatcher runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    /// Lists the terminals Orca has open and types into them.
    pub orca: Tool,
}

impl Default for Tools {
    fn default() -> Self {
        Tools { orca: Tool::orca() }
    }
}

/// How long the dispatcher waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timing {
    /// For a command to finish, on top of what it was asked to wait itself.
    pub command: Duration,
    /// For the agent to be seen starting a turn with a prompt, after it was
    /// typed. Nothing is typed again when this runs out: the receipt then
    /// says the start wasn't seen.
    pub wait_submit: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            command: Duration::from_secs(30),
            wait_submit: Duration::from_secs(5),
        }
    }
}

/// What an open session is doing.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// Its turn is over and it waits for a prompt.
    Ready,
    /// In the middle of a turn. What is typed now is the agent's to queue.
    Working,
    /// Asking its user something: a permission, a choice. Nothing is typed
    /// into a session in this state.
    Asking,
    #[default]
    Unknown,
}

impl State {
    /// From the word Orca uses for an agent's state.
    #[must_use]
    pub fn from_orca(state: &str) -> Self {
        match state {
            "done" | "idle" => State::Ready,
            "working" => State::Working,
            "waiting" | "blocked" => State::Asking,
            _ => State::Unknown,
        }
    }

    /// The word shown to people.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::Working => "working",
            State::Asking => "asking",
            State::Unknown => "",
        }
    }
}

/// A coding-agent session that is open in a window on this computer.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Session {
    /// The program that hosts its window: `orca`.
    pub host: String,
    /// The host's handle for that window. It lasts until the host or the
    /// terminal restarts, so list again rather than keep one for later.
    pub handle: String,
    /// The agent, as the host names it: `claude`, `codex`, `kimi`, …
    pub agent: String,
    /// What the session is called in the host.
    pub name: String,
    /// The project folder it works in.
    pub cwd: String,
    #[serde(default)]
    pub branch: Option<String>,
    pub state: State,
    /// The start of the last prompt it was given, to tell sessions apart.
    #[serde(default)]
    pub last_prompt: Option<String>,
    /// When it last changed, in milliseconds since 1970.
    #[serde(default)]
    pub updated_at: Option<u64>,
}

/// The sessions open on this computer, the one that changed last first.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    /// Goes up when a field changes meaning or goes away.
    pub version: u32,
    pub host: String,
    pub sessions: Vec<Session>,
    /// What a reader should know about the list: what it may leave out.
    pub notes: Vec<String>,
}

/// What became of a prompt that was typed into a session.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub host: String,
    pub handle: String,
    /// The agent the host typed it to, in the host's word for it.
    pub agent: String,
    /// What the session is called, as the list showed it.
    pub name: String,
    /// The project folder it works in.
    pub cwd: String,
    /// What the session was doing just before.
    pub state_before: State,
    /// Whether the agent was seen starting a turn with it. `false` is not a
    /// failure: the prompt was typed and entered, and the start wasn't seen
    /// in the time allowed, as when it waits behind what the agent is doing.
    pub started: bool,
    /// What the host said about it, in its words.
    pub notes: Vec<String>,
}

/// The short form of a handle, as lists show it: the first eight characters
/// after the host's prefix.
#[must_use]
pub fn short_id(handle: &str) -> &str {
    let id = handle.strip_prefix("term_").unwrap_or(handle);
    id.char_indices().nth(8).map_or(id, |(at, _)| &id[..at])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_is_shown_by_its_first_eight_characters() {
        assert_eq!(
            short_id("term_5e47b923-0963-451d-ba71-d0756e7ba2cc"),
            "5e47b923"
        );
        assert_eq!(short_id("5e47b923-0963"), "5e47b923");
        assert_eq!(short_id("term_abc"), "abc");
        assert_eq!(short_id(""), "");
    }

    #[test]
    fn orcas_words_for_an_agent_become_three_states() {
        assert_eq!(State::from_orca("done"), State::Ready);
        assert_eq!(State::from_orca("idle"), State::Ready);
        assert_eq!(State::from_orca("working"), State::Working);
        assert_eq!(State::from_orca("waiting"), State::Asking);
        assert_eq!(State::from_orca("blocked"), State::Asking);
        // A word this doesn't know is not taken for ready.
        assert_eq!(State::from_orca("sleeping"), State::Unknown);
        assert_eq!(State::from_orca(""), State::Unknown);
    }

    // The paths in these tests are written the Unix way.
    #[cfg(unix)]
    fn found(
        bin_dir: Option<&str>,
        path: Option<&str>,
        home: Option<&str>,
        there: &[&str],
    ) -> String {
        let tool = find_orca(
            bin_dir.map(Path::new),
            path.map(OsStr::new),
            home.map(Path::new),
            &|candidate| there.iter().any(|path| Path::new(path) == candidate),
        );
        tool.program.to_string_lossy().into_owned()
    }

    #[cfg(unix)]
    #[test]
    fn orcas_command_is_looked_for_where_orca_keeps_it() {
        let in_app = "/Applications/Orca.app/Contents/Resources/bin/orca";
        let in_home = "/Users/me/Applications/Orca.app/Contents/Resources/bin/orca";
        // Where Orca says it is comes first, then the PATH, then the app.
        assert_eq!(
            found(
                Some("/opt/orca/bin"),
                Some("/usr/bin:/usr/local/bin"),
                Some("/Users/me"),
                &["/opt/orca/bin/orca", "/usr/local/bin/orca", in_app]
            ),
            "/opt/orca/bin/orca"
        );
        assert_eq!(
            found(
                None,
                Some("/usr/bin:/usr/local/bin"),
                Some("/Users/me"),
                &["/usr/local/bin/orca", in_app]
            ),
            "/usr/local/bin/orca"
        );
        assert_eq!(
            found(None, Some("/usr/bin"), Some("/Users/me"), &[in_app]),
            in_app
        );
        assert_eq!(
            found(None, Some("/usr/bin"), Some("/Users/me"), &[in_home]),
            in_home
        );
        // Found nowhere: the bare name, which fails as not installed.
        assert_eq!(found(None, None, None, &[]), "orca");
    }

    #[cfg(unix)]
    #[test]
    fn an_empty_path_entry_is_not_this_folder() {
        let tool = find_orca(None, Some(OsStr::new(":/usr/bin:")), None, &|candidate| {
            assert!(candidate.is_absolute(), "looked at {candidate:?}");
            false
        });
        assert_eq!(tool, Tool::named("orca"));
    }
}
