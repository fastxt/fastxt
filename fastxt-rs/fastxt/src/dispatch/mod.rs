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

//! Type a prepared prompt into a coding-agent session that is open on this
//! computer.
//!
//! A session here is one its user started and can see: an agent running in a
//! terminal window. [`Dispatcher::sessions`] lists the ones that are open,
//! and [`Dispatcher::send`] types a prompt into one, as if its user had
//! typed it there. Nothing is started, and nothing runs out of sight.
//!
//! Typing into a window takes the program that hosts it. For now that is
//! [Orca](https://www.onorca.dev), through its own command ([`orca`]): any
//! agent that runs in one of its terminals is a session.
//!
//! Three rules hold:
//!
//! - **No shell.** The prompt is one argument of the host's command.
//! - **A prompt never answers a question.** While an agent shows a
//!   permission or trust question, nothing is typed into its session: text
//!   followed by Enter would pick an answer. What a session may do stays
//!   what its user set in that session.
//! - **Only an agent gets a prompt.** A terminal with a plain shell in it
//!   would run the text as a command, so a session is looked up again right
//!   before every send, and a receipt that names no agent is an error.
//!
//! This module has no part in the notes database: a caller looks the prompt
//! up and passes the text.

pub mod error;
pub mod exec;
pub mod model;
pub mod orca;

pub use error::{Error, Result};
pub use exec::CommandSpec;
pub use model::{Listing, Receipt, Session, State, Timing, Tool, Tools, short_id};

/// The longest prompt that is sent. It travels as one argument of a command,
/// and Windows allows a whole command line some 32,000 characters.
#[cfg(windows)]
pub const MAX_PROMPT_BYTES: usize = 30_000;
/// The longest prompt that is sent. It travels as one argument of a command.
#[cfg(not(windows))]
pub const MAX_PROMPT_BYTES: usize = 100_000;

/// A send that passed its checks: where it would go and the command it
/// would run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub session: Session,
    /// The whole command, prompt included.
    pub command: CommandSpec,
}

/// Lists open sessions and types prompts into them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dispatcher {
    pub tools: Tools,
    pub timing: Timing,
}

impl Dispatcher {
    /// A dispatcher that looks for Orca's command where Orca keeps it.
    #[must_use]
    pub fn new() -> Self {
        Dispatcher::default()
    }

    /// The agent sessions open on this computer.
    ///
    /// # Errors
    /// [`Error::NotInstalled`] when the host's command isn't found, and
    /// [`Error::Failed`], [`Error::Timeout`] or [`Error::Unreadable`] from
    /// running it.
    pub fn sessions(&self) -> Result<Listing> {
        orca::sessions(&self.tools, &self.timing)
    }

    /// Runs every check a send does and returns the command it would run.
    /// Reads the list of sessions; types nothing.
    ///
    /// # Errors
    /// As [`Dispatcher::send`], short of running the command.
    pub fn plan(&self, handle: &str, prompt: &str) -> Result<Plan> {
        let prompt = check_prompt(prompt)?;
        check_handle(handle)?;
        // Looked up now, not taken from an earlier list: the agent may have
        // ended since, and its terminal would then be a shell.
        let listing = self.sessions()?;
        let Some(session) = listing.sessions.into_iter().find(|s| s.handle == handle) else {
            return Err(Error::Gone(short_id(handle).to_string()));
        };
        if session.state == State::Asking {
            return Err(Error::Asking(short_id(handle).to_string()));
        }
        let command = orca::send_spec(&self.tools, &self.timing, handle, prompt);
        Ok(Plan { session, command })
    }

    /// Types the prompt into the session and presses Enter.
    ///
    /// # Errors
    /// - [`Error::Invalid`] for an empty or oversized prompt, or a handle
    ///   that isn't one.
    /// - [`Error::Gone`] when the session is no longer open.
    /// - [`Error::Asking`] when its agent is showing a question; nothing was
    ///   typed.
    /// - [`Error::NotAnAgent`] when the prompt reached a shell instead.
    /// - [`Error::NotInstalled`], [`Error::Timeout`], [`Error::Failed`] and
    ///   [`Error::Unreadable`] from running the host's command.
    pub fn send(&self, handle: &str, prompt: &str) -> Result<Receipt> {
        let plan = self.plan(handle, prompt)?;
        orca::send(&self.timing, &plan.session, &plan.command)
    }
}

/// The session a handle, or the start of one, names. The start may leave
/// out the host's `term_` prefix, as lists show it.
///
/// # Errors
/// [`Error::NoSession`] when none does and [`Error::Ambiguous`] when the
/// start fits more than one.
pub fn resolve<'a>(sessions: &'a [Session], wanted: &str) -> Result<&'a Session> {
    let wanted = wanted.trim();
    if wanted.is_empty() {
        return Err(Error::NoSession("an empty ID".to_string()));
    }
    if let Some(exact) = sessions.iter().find(|s| s.handle == wanted) {
        return Ok(exact);
    }
    let starts: Vec<&Session> = sessions
        .iter()
        .filter(|s| {
            s.handle.starts_with(wanted)
                || s.handle
                    .strip_prefix("term_")
                    .is_some_and(|id| id.starts_with(wanted))
        })
        .collect();
    match starts.as_slice() {
        [] => Err(Error::NoSession(wanted.to_string())),
        [one] => Ok(one),
        many => Err(Error::Ambiguous {
            wanted: wanted.to_string(),
            count: many.len(),
        }),
    }
}

/// A prompt as it is sent: without the blank space around it, and refused
/// when nothing is left, when it is too long for an argument, or when it
/// holds a NUL, which no argument can.
fn check_prompt(prompt: &str) -> Result<&str> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(Error::Invalid("the prompt is empty".to_string()));
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(Error::Invalid(format!(
            "the prompt is {} bytes, and at most {MAX_PROMPT_BYTES} can be sent",
            prompt.len()
        )));
    }
    if prompt.contains('\0') {
        return Err(Error::Invalid(
            "the prompt holds a NUL character".to_string(),
        ));
    }
    Ok(prompt)
}

/// A handle is letters, digits, hyphens and underscores: anything else isn't
/// one, and one that starts with a hyphen would be read as an option.
fn check_handle(handle: &str) -> Result<()> {
    let ok = !handle.is_empty()
        && handle.len() <= 128
        && !handle.starts_with('-')
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "{handle:?} is not a session handle"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_is_sent_without_the_space_around_it() {
        assert_eq!(
            check_prompt("  fix the test \n").expect("ok"),
            "fix the test"
        );
        assert_eq!(check_prompt("修复登录测试\n").expect("ok"), "修复登录测试");
        // The lines inside it stay.
        assert_eq!(check_prompt("one\ntwo\n").expect("ok"), "one\ntwo");
    }

    #[test]
    fn an_empty_oversized_or_nul_prompt_is_refused() {
        assert!(matches!(check_prompt(" \n\t"), Err(Error::Invalid(_))));
        assert!(matches!(check_prompt("a\0b"), Err(Error::Invalid(_))));
        let long = "x".repeat(MAX_PROMPT_BYTES + 1);
        assert!(matches!(check_prompt(&long), Err(Error::Invalid(_))));
        assert!(check_prompt(&"x".repeat(MAX_PROMPT_BYTES)).is_ok());
    }

    #[test]
    fn a_handle_cannot_pass_for_an_option() {
        assert!(check_handle("term_5e47b923-0963-451d-ba71-d0756e7ba2cc").is_ok());
        for bad in ["", "--terminal", "-x", "a b", "a;b", "../x", "term_\n"] {
            assert!(check_handle(bad).is_err(), "{bad:?} was accepted");
        }
        assert!(check_handle(&"a".repeat(129)).is_err());
    }

    fn session(handle: &str) -> Session {
        Session {
            host: orca::HOST.to_string(),
            handle: handle.to_string(),
            agent: "claude".to_string(),
            ..Session::default()
        }
    }

    #[test]
    fn a_handle_or_its_start_names_one_session() {
        let sessions = [
            session("term_5e47b923-0963-451d-ba71-d0756e7ba2cc"),
            session("term_5e47ffff-0000-4000-8000-000000000000"),
            session("term_aaaaaaaa-0000-4000-8000-000000000001"),
        ];
        let found = |wanted: &str| resolve(&sessions, wanted).map(|s| s.handle.as_str());
        assert_eq!(
            found("aaaaaaaa").expect("the short form"),
            "term_aaaaaaaa-0000-4000-8000-000000000001"
        );
        assert_eq!(
            found(" term_aaaa ").expect("with the prefix"),
            "term_aaaaaaaa-0000-4000-8000-000000000001"
        );
        assert_eq!(
            found("term_5e47b923-0963-451d-ba71-d0756e7ba2cc").expect("the whole handle"),
            "term_5e47b923-0963-451d-ba71-d0756e7ba2cc"
        );
        match found("5e47") {
            Err(Error::Ambiguous { count, .. }) => assert_eq!(count, 2),
            other => panic!("expected Ambiguous, got {other:?}"),
        }
        assert!(matches!(found("ffff"), Err(Error::NoSession(_))));
        assert!(matches!(found(""), Err(Error::NoSession(_))));
    }

    #[test]
    fn a_bad_prompt_or_handle_is_refused_before_anything_runs() {
        // No such program: reaching it would be a different error.
        let dispatcher = Dispatcher {
            tools: Tools {
                orca: Tool::named("fastxt-no-such-orca"),
            },
            timing: Timing::default(),
        };
        assert!(matches!(
            dispatcher.send("term_a", "  \n"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            dispatcher.send("--all", "p"),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            dispatcher.send("term_a", "p"),
            Err(Error::NotInstalled(_))
        ));
    }
}
