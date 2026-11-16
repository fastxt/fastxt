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

//! The dispatcher as the sync server's [`AgentHost`]: what lets a paired
//! phone see the sessions open here and type a note into one.

use super::Dispatcher;
use super::error::Error;
use super::model::Session;
use fastxt_core::model::{AgentSession, SendRefusal};
use fastxt_core::sync::{AgentHost, Typed};

impl AgentHost for Dispatcher {
    fn sessions(&self) -> Result<Vec<AgentSession>, SendRefusal> {
        let listing = Dispatcher::sessions(self).map_err(refusal)?;
        Ok(listing.sessions.into_iter().map(wire).collect())
    }

    fn send(&self, handle: &str, text: &str) -> Result<Typed, SendRefusal> {
        let receipt = Dispatcher::send(self, handle, text).map_err(refusal)?;
        Ok(Typed {
            host: receipt.host,
            agent: receipt.agent,
            session: receipt.name,
            cwd: receipt.cwd,
            started: receipt.started,
        })
    }
}

/// A session as it travels to a paired device, and as `fastxt sessions`
/// shows one, here or there.
#[must_use]
pub fn wire(session: Session) -> AgentSession {
    AgentSession {
        host: session.host,
        handle: session.handle,
        agent: session.agent,
        name: session.name,
        cwd: session.cwd,
        branch: session.branch,
        state: session.state.word().to_string(),
        last_prompt: session.last_prompt,
        updated_at: session.updated_at,
    }
}

/// Why nothing (or the wrong thing) was typed, for a paired device: a reason
/// its program can act on and the message for its user.
fn refusal(error: Error) -> SendRefusal {
    let reason = match &error {
        Error::NotInstalled(_) => SendRefusal::UNAVAILABLE,
        Error::Gone(_) | Error::NoSession(_) | Error::Ambiguous { .. } => SendRefusal::GONE,
        Error::Asking(_) => SendRefusal::ASKING,
        Error::NotAnAgent(_) => SendRefusal::NOT_AN_AGENT,
        Error::Invalid(_) => SendRefusal::INVALID,
        Error::Failed { .. } | Error::Timeout { .. } | Error::Unreadable { .. } | Error::Io(_) => {
            SendRefusal::FAILED
        }
    };
    SendRefusal::new(reason, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::model::State;

    #[test]
    fn a_session_travels_with_its_state_as_a_word() {
        let session = Session {
            host: "orca".into(),
            handle: "term_a".into(),
            agent: "claude".into(),
            name: "Fix login".into(),
            cwd: "/code/api".into(),
            branch: Some("main".into()),
            state: State::Asking,
            last_prompt: Some("Delete the cache".into()),
            updated_at: Some(7),
        };
        let wired = wire(session);
        assert_eq!(wired.state, "asking");
        assert_eq!(wired.handle, "term_a");
        assert_eq!(wired.branch.as_deref(), Some("main"));
        assert_eq!(wired.updated_at, Some(7));
        assert_eq!(wire(Session::default()).state, "");
    }

    #[test]
    fn every_error_becomes_a_reason_a_phone_can_act_on() {
        let cases = [
            (Error::NotInstalled("orca".into()), SendRefusal::UNAVAILABLE),
            (Error::Gone("aaaa".into()), SendRefusal::GONE),
            (Error::NoSession("aaaa".into()), SendRefusal::GONE),
            (Error::Asking("aaaa".into()), SendRefusal::ASKING),
            (Error::NotAnAgent("aaaa".into()), SendRefusal::NOT_AN_AGENT),
            (
                Error::Invalid("the prompt is empty".into()),
                SendRefusal::INVALID,
            ),
            (
                Error::Timeout {
                    program: "orca".into(),
                    seconds: 30,
                },
                SendRefusal::FAILED,
            ),
        ];
        for (error, reason) in cases {
            let message = error.to_string();
            let refusal = refusal(error);
            assert_eq!(refusal.reason, reason);
            assert_eq!(refusal.message, message, "the dispatcher's own words");
        }
    }
}
