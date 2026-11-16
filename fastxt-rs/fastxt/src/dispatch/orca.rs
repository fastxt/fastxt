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

//! Orca: the agent sessions open in its terminals, and typing a prompt into
//! one of them.
//!
//! What this is written against, each line checked with Orca 1.4.258:
//!
//! - `orca worktree ps --json` lists, per worktree, the agents Orca knows:
//!   `paneKey` (`<tabId>:<leafId>`), `agentType`, `state` (`working`, `done`,
//!   `waiting`, …), `conversationName`, the last `prompt`, `updatedAt`.
//! - `orca terminal list --json` lists the live terminals: `handle`, `tabId`,
//!   `leafId`, `worktreePath`, `branch`, `title`, `connected`, `writable`.
//!   Its `agentIdentity` is missing for most terminals that do run an agent,
//!   so which sessions there are comes from `worktree ps`, and only the
//!   handle from here.
//! - `orca terminal send --terminal <handle> --text <prompt> --enter
//!   --wait-submit <seconds> --json`, with an agent running in the terminal,
//!   pastes the prompt as one block and presses Enter. `send.prompt.stages`
//!   is `input_accepted`, then `turn_started` once the turn was seen to
//!   start, which it isn't always; `send.prompt.provider` names the agent.
//! - While the agent shows a permission or trust question, that command
//!   types nothing and fails with `agent_prompt_blocked`: exit 1, and
//!   `{"ok": false, "error": {"code": …, "message": …}}` on stdout.
//! - With no agent in the terminal, the same command types the text into the
//!   shell and presses Enter, and `provider` is `unsupported`. Orca has no
//!   option to refuse that, so a session is looked up again right before
//!   every send. What is left is the instant in between.
//! - A handle that doesn't exist fails with `terminal_handle_stale`, and a
//!   closed terminal's with `terminal_not_writable`.
//! - A terminal Orca reports as `orphaned` is running but not bound to a
//!   pane of its window, so it may not be on screen; its `tabId` and
//!   `leafId` are then made up (`pty:…`) and its pane is in
//!   `recordedPaneKey`. Seen with one `orca terminal create` that Orca's
//!   window didn't adopt. Such a terminal isn't listed: a session here is
//!   one its user can see.

use super::error::{Error, Result};
use super::exec::{self, CommandSpec, Output};
use super::model::{Listing, Receipt, Session, State, Timing, Tools, short_id};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;

/// The name sessions and receipts carry for this host.
pub const HOST: &str = "orca";

/// How much of a session's last prompt a list carries.
const LAST_PROMPT_CHARS: usize = 120;

#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    ok: bool,
    #[serde(default = "Option::default")]
    result: Option<T>,
    #[serde(default)]
    error: Option<HostError>,
}

#[derive(Deserialize, Debug, Default)]
struct HostError {
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Deserialize, Default)]
struct TerminalList {
    #[serde(default)]
    terminals: Vec<Terminal>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Terminal {
    handle: String,
    #[serde(default)]
    tab_id: Option<String>,
    #[serde(default)]
    leaf_id: Option<String>,
    #[serde(default)]
    worktree_path: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    connected: bool,
    #[serde(default)]
    writable: bool,
    /// Running, but not bound to a pane of Orca's window.
    #[serde(default)]
    orphaned: bool,
}

#[derive(Deserialize, Default)]
struct Processes {
    #[serde(default)]
    worktrees: Vec<Worktree>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct Worktree {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    agents: Vec<Agent>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Agent {
    #[serde(default)]
    pane_key: Option<String>,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    conversation_name: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    updated_at: Option<u64>,
    #[serde(default)]
    activity: Option<Activity>,
}

#[derive(Deserialize)]
struct Activity {
    #[serde(default)]
    title: Option<String>,
}

#[derive(Deserialize)]
struct SendResult {
    #[serde(default)]
    send: Option<Sent>,
    #[serde(default)]
    warnings: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Sent {
    #[serde(default)]
    accepted: bool,
    #[serde(default)]
    prompt: Option<Prompt>,
}

#[derive(Deserialize)]
struct Prompt {
    #[serde(default)]
    stages: Vec<String>,
    #[serde(default)]
    provider: Option<String>,
}

/// What Orca's command answered: its result, or the error it reported.
/// Orca prints both as JSON on stdout, with exit 1 for an error.
fn answer<T: DeserializeOwned>(
    spec: &CommandSpec,
    output: &Output,
) -> Result<std::result::Result<T, HostError>> {
    let parsed: std::result::Result<Envelope<T>, _> = serde_json::from_str(output.stdout.trim());
    match parsed {
        Ok(Envelope {
            ok: true,
            result: Some(result),
            ..
        }) => Ok(Ok(result)),
        Ok(Envelope {
            error: Some(error), ..
        }) => Ok(Err(error)),
        // No JSON, or JSON that is neither: a failure in its own words when
        // it failed, and something this can't read when it didn't.
        _ if !output.success() => Err(Error::Failed {
            program: spec.name(),
            message: output.complaint(),
        }),
        Ok(_) => Err(Error::Unreadable {
            program: spec.name(),
            message: "it said neither a result nor an error".to_string(),
        }),
        Err(error) => Err(Error::Unreadable {
            program: spec.name(),
            message: error.to_string(),
        }),
    }
}

/// An error Orca reported that isn't one this knows how to name.
fn failed(spec: &CommandSpec, error: HostError) -> Error {
    Error::Failed {
        program: spec.name(),
        message: error
            .message
            .or(error.code)
            .unwrap_or_else(|| "it reported an error without saying which".to_string()),
    }
}

fn non_empty(text: Option<&String>) -> Option<&str> {
    text.map(|text| text.trim()).filter(|text| !text.is_empty())
}

/// The start of a prompt on one line.
fn snippet(prompt: &str) -> Option<String> {
    let line = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.is_empty() {
        return None;
    }
    Some(if line.chars().count() > LAST_PROMPT_CHARS {
        let cut: String = line.chars().take(LAST_PROMPT_CHARS).collect();
        format!("{cut}…")
    } else {
        line
    })
}

/// The agents that have a terminal to type into, each with that terminal's
/// handle; the one that changed last first.
fn join(processes: Processes, terminals: &TerminalList) -> Vec<Session> {
    let by_pane: HashMap<String, &Terminal> = terminals
        .terminals
        .iter()
        .filter(|terminal| terminal.connected && terminal.writable && !terminal.orphaned)
        .filter_map(|terminal| {
            let pane = format!(
                "{}:{}",
                terminal.tab_id.as_deref()?,
                terminal.leaf_id.as_deref()?
            );
            Some((pane, terminal))
        })
        .collect();
    let mut sessions: Vec<Session> = Vec::new();
    for worktree in processes.worktrees {
        for agent in worktree.agents {
            // An agent Orca remembers without a live terminal isn't open.
            let Some(terminal) = agent.pane_key.as_ref().and_then(|pane| by_pane.get(pane)) else {
                continue;
            };
            let Some(kind) = non_empty(agent.agent_type.as_ref()) else {
                continue;
            };
            let name = non_empty(agent.conversation_name.as_ref())
                .or_else(|| non_empty(agent.activity.as_ref().and_then(|a| a.title.as_ref())))
                .or_else(|| non_empty(terminal.title.as_ref()))
                .unwrap_or_default();
            let session = Session {
                host: HOST.to_string(),
                handle: terminal.handle.clone(),
                agent: kind.to_string(),
                name: name.to_string(),
                cwd: non_empty(terminal.worktree_path.as_ref())
                    .or_else(|| non_empty(worktree.path.as_ref()))
                    .unwrap_or_default()
                    .to_string(),
                // Orca gives the whole ref, `refs/heads/main`.
                branch: non_empty(terminal.branch.as_ref())
                    .or_else(|| non_empty(worktree.branch.as_ref()))
                    .map(|branch| branch.strip_prefix("refs/heads/").unwrap_or(branch))
                    .map(str::to_string),
                state: State::from_orca(agent.state.as_deref().unwrap_or_default()),
                last_prompt: agent.prompt.as_deref().and_then(snippet),
                updated_at: agent.updated_at,
            };
            // One terminal, one session: of two agents Orca files under the
            // same pane, the one that changed last is the one on screen.
            match sessions.iter_mut().find(|s| s.handle == session.handle) {
                Some(seen) if seen.updated_at < session.updated_at => *seen = session,
                Some(_) => {}
                None => sessions.push(session),
            }
        }
    }
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions
}

fn run<T: DeserializeOwned>(tools: &Tools, timing: &Timing, args: [&str; 3]) -> Result<T> {
    let spec = CommandSpec::new(&tools.orca).args(args);
    let output = exec::run(&spec, timing.command)?;
    answer(&spec, &output)?.map_err(|error| failed(&spec, error))
}

/// The agent sessions open in Orca's terminals.
pub(crate) fn sessions(tools: &Tools, timing: &Timing) -> Result<Listing> {
    let processes: Processes = run(tools, timing, ["worktree", "ps", "--json"])?;
    let terminals: TerminalList = run(tools, timing, ["terminal", "list", "--json"])?;
    let mut notes = Vec::new();
    if processes.truncated || terminals.truncated {
        notes.push("Orca cut its list short, so some open sessions may be missing".to_string());
    }
    Ok(Listing {
        version: 1,
        host: HOST.to_string(),
        sessions: join(processes, &terminals),
        notes,
    })
}

/// The command that types `prompt` into the terminal and presses Enter.
pub(crate) fn send_spec(tools: &Tools, timing: &Timing, handle: &str, prompt: &str) -> CommandSpec {
    let spec = CommandSpec::new(&tools.orca)
        .args(["terminal", "send", "--terminal", handle])
        // `--text=…`, one word: a prompt that starts with a hyphen stays
        // the text.
        .arg(format!("--text={prompt}"))
        .arg("--enter");
    let wait = timing.wait_submit.as_secs();
    let spec = if wait > 0 {
        spec.args(["--wait-submit".to_string(), wait.to_string()])
    } else {
        spec
    };
    spec.arg("--json")
}

/// Runs a send and reads what became of the prompt.
pub(crate) fn send(timing: &Timing, session: &Session, spec: &CommandSpec) -> Result<Receipt> {
    let output = exec::run(spec, timing.command + timing.wait_submit)?;
    let short = short_id(&session.handle).to_string();
    let result: SendResult = match answer(spec, &output)? {
        Ok(result) => result,
        Err(error) => {
            return Err(match error.code.as_deref() {
                Some("agent_prompt_blocked") => Error::Asking(short),
                Some("terminal_handle_stale" | "terminal_not_writable") => Error::Gone(short),
                _ => failed(spec, error),
            });
        }
    };
    let Some(sent) = result.send.filter(|sent| sent.accepted) else {
        return Err(Error::Failed {
            program: spec.name(),
            message: "it did not take the input".to_string(),
        });
    };
    let mut notes = result.warnings.unwrap_or_default();
    let (agent, started) = match sent.prompt {
        Some(Prompt { provider, .. }) if provider.as_deref() == Some("unsupported") => {
            return Err(Error::NotAnAgent(short));
        }
        Some(Prompt { stages, provider }) => (
            provider.unwrap_or_else(|| session.agent.clone()),
            stages.iter().any(|stage| stage == "turn_started"),
        ),
        // An Orca that doesn't say who took the input can't say more.
        None => {
            notes.push("Orca did not say what took the input".to_string());
            (session.agent.clone(), false)
        }
    };
    Ok(Receipt {
        host: HOST.to_string(),
        handle: session.handle.clone(),
        agent,
        name: session.name.clone(),
        cwd: session.cwd.clone(),
        state_before: session.state,
        started,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::model::Tool;

    fn processes(json: &str) -> Processes {
        serde_json::from_str(json).expect("the processes parse")
    }

    fn terminals(json: &str) -> TerminalList {
        serde_json::from_str(json).expect("the terminals parse")
    }

    // The shapes Orca 1.4.258 printed, with made-up values.
    const PS: &str = r#"{"worktrees": [
      {"path": "/code/api", "branch": "main", "displayName": "api", "agents": [
        {"paneKey": "tab-1:leaf-1", "parentPaneKey": null, "state": "done", "agentType": "claude",
         "prompt": "Fix the flaky\n  login test", "conversationName": "Fix login", "hasLivePty": true,
         "updatedAt": 200, "activity": {"listed": true, "status": "done", "title": "Fix login"}},
        {"paneKey": "tab-2:leaf-1", "state": "working", "agentType": "kimi", "prompt": "",
         "conversationName": null, "hasLivePty": true, "updatedAt": 300,
         "activity": {"status": "working", "title": "Port the parser"}},
        {"paneKey": "tab-9:leaf-9", "state": "done", "agentType": "claude", "prompt": "old",
         "conversationName": "Closed last week", "hasLivePty": null, "updatedAt": 900}
      ]},
      {"path": "/code/web", "branch": "feat/x", "agents": [
        {"paneKey": "tab-3:leaf-2", "state": "waiting", "agentType": "antigravity",
         "conversationName": "", "updatedAt": 100, "activity": null}
      ]},
      {"path": "/code/empty", "branch": "main", "agents": []}
    ], "truncated": false}"#;

    const TERMINALS: &str = r#"{"terminals": [
      {"handle": "term_aaaaaaaa-0000-4000-8000-000000000001", "tabId": "tab-1", "leafId": "leaf-1",
       "worktreePath": "/code/api", "branch": "refs/heads/main", "title": "✳ Fix login", "connected": true,
       "writable": true, "agentIdentity": "claude", "orphaned": false, "lastOutputAt": 5},
      {"handle": "term_bbbbbbbb-0000-4000-8000-000000000002", "tabId": "tab-2", "leafId": "leaf-1",
       "worktreePath": "/code/api", "branch": "main", "title": "kimi", "connected": true, "writable": true},
      {"handle": "term_cccccccc-0000-4000-8000-000000000003", "tabId": "tab-3", "leafId": "leaf-2",
       "worktreePath": "/code/web", "branch": "feat/x", "title": "agy", "connected": true, "writable": true},
      {"handle": "term_dddddddd-0000-4000-8000-000000000004", "tabId": "tab-4", "leafId": "leaf-1",
       "worktreePath": "/code/api", "branch": "main", "title": "zsh", "connected": true, "writable": true}
    ], "truncated": false}"#;

    #[test]
    fn open_sessions_are_the_agents_that_have_a_terminal() {
        let sessions = join(processes(PS), &terminals(TERMINALS));
        let seen: Vec<(&str, &str, &str)> = sessions
            .iter()
            .map(|s| (short_id(&s.handle), s.agent.as_str(), s.name.as_str()))
            .collect();
        // Newest first; the agent whose terminal is gone and the terminal
        // that only runs a shell are not sessions.
        assert_eq!(
            seen,
            [
                ("bbbbbbbb", "kimi", "Port the parser"),
                ("aaaaaaaa", "claude", "Fix login"),
                ("cccccccc", "antigravity", "agy"),
            ]
        );
        assert_eq!(sessions[0].state, State::Working);
        assert_eq!(sessions[1].state, State::Ready);
        assert_eq!(sessions[2].state, State::Asking);
        assert_eq!(sessions[1].cwd, "/code/api");
        assert_eq!(sessions[2].branch.as_deref(), Some("feat/x"));
        assert_eq!(
            sessions[1].branch.as_deref(),
            Some("main"),
            "the branch, not the whole ref Orca gives"
        );
        assert_eq!(
            sessions[1].last_prompt.as_deref(),
            Some("Fix the flaky login test"),
            "one line"
        );
        assert_eq!(sessions[0].last_prompt, None, "an empty prompt is none");
        assert!(sessions.iter().all(|s| s.host == HOST));
    }

    #[test]
    fn a_terminal_that_cannot_be_typed_into_is_not_a_session() {
        let closed = TERMINALS.replacen("\"writable\": true", "\"writable\": false", 1);
        let sessions = join(processes(PS), &terminals(&closed));
        assert!(sessions.iter().all(|s| s.agent != "claude"));
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn a_terminal_that_is_not_bound_to_a_pane_on_screen_is_not_a_session() {
        // What Orca 1.4.258 printed for a terminal its window didn't adopt:
        // made-up tab and leaf, its pane recorded beside them. Its agent's
        // row names that recorded pane.
        let ps = r#"{"worktrees": [{"path": "/code/api", "agents": [
          {"paneKey": "tab-7:leaf-7", "state": "done", "agentType": "claude",
           "conversationName": "Off screen", "hasLivePty": true, "updatedAt": 5}
        ]}]}"#;
        let terminals_json = r#"{"terminals": [
          {"handle": "term_ffffffff-0000-4000-8000-000000000007", "orphaned": true,
           "recordedPaneKey": "tab-7:leaf-7",
           "tabId": "pty:w::/code/api@@a9457689", "leafId": "pty:w::/code/api@@a9457689",
           "worktreePath": "/code/api", "connected": true, "writable": true, "agentIdentity": "claude"}
        ]}"#;
        assert!(join(processes(ps), &terminals(terminals_json)).is_empty());
        // Bound to that pane, the same terminal is a session.
        let bound = terminals_json
            .replace("\"orphaned\": true", "\"orphaned\": false")
            .replace(
                "pty:w::/code/api@@a9457689\", \"leafId\": \"pty:w::/code/api@@a9457689",
                "tab-7\", \"leafId\": \"leaf-7",
            );
        assert_eq!(join(processes(ps), &terminals(&bound)).len(), 1);
    }

    #[test]
    fn two_agents_under_one_pane_are_one_session_the_newer() {
        let ps = r#"{"worktrees": [{"path": "/code/api", "agents": [
          {"paneKey": "tab-1:leaf-1", "state": "done", "agentType": "claude", "conversationName": "older", "updatedAt": 1},
          {"paneKey": "tab-1:leaf-1", "state": "working", "agentType": "claude", "conversationName": "newer", "updatedAt": 2}
        ]}]}"#;
        let sessions = join(processes(ps), &terminals(TERMINALS));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].name, "newer");
        assert_eq!(sessions[0].state, State::Working);
    }

    #[test]
    fn a_long_last_prompt_is_cut() {
        let long = "word ".repeat(100);
        let cut = snippet(&long).expect("a snippet");
        assert_eq!(cut.chars().count(), LAST_PROMPT_CHARS + 1);
        assert!(cut.ends_with('…'));
        assert_eq!(snippet(" \n\t "), None);
        assert_eq!(
            snippet("修复 登录\n测试").as_deref(),
            Some("修复 登录 测试")
        );
    }

    fn tools() -> Tools {
        Tools {
            orca: Tool::named("orca"),
        }
    }

    #[test]
    fn a_send_is_one_guarded_command_with_the_prompt_as_one_word() {
        let spec = send_spec(
            &tools(),
            &Timing::default(),
            "term_aaaaaaaa-0000-4000-8000-000000000001",
            "--help me fix\nthe login test",
        );
        assert_eq!(
            spec.args,
            [
                "terminal",
                "send",
                "--terminal",
                "term_aaaaaaaa-0000-4000-8000-000000000001",
                "--text=--help me fix\nthe login test",
                "--enter",
                "--wait-submit",
                "5",
                "--json"
            ]
        );
        // Text and Enter in one command is what Orca guards; never apart.
        assert!(spec.args.contains(&"--enter".to_string()));
    }

    #[test]
    fn no_wait_leaves_the_option_out() {
        let timing = Timing {
            wait_submit: std::time::Duration::ZERO,
            ..Timing::default()
        };
        let spec = send_spec(&tools(), &timing, "term_a", "p");
        assert!(!spec.args.iter().any(|arg| arg == "--wait-submit"));
        assert_eq!(spec.args.last().map(String::as_str), Some("--json"));
    }
}
