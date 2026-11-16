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

//! The dispatcher against a stand-in for `orca`: a shell script that writes
//! down how it was called and prints what the real command printed when
//! this was written (Orca 1.4.258). No terminal is typed into.

#![cfg(unix)]

use fastxt::dispatch::{Dispatcher, Error, State, Timing, Tool, Tools, resolve};
use std::path::PathBuf;
use std::time::Duration;

// Newlines inside an argument are written as the unit separator, so one
// argument stays one line of the log.
const ORCA_SH: &str = r#"
here=$(dirname "$0")
{
  echo "--- call"
  for a in "$@"; do printf 'arg=%s\n' "$(printf '%s' "$a" | tr '\n' '\037')"; done
} >> "$here/orca.calls"
case "$1 $2" in
  "worktree ps") name=ps ;;
  "terminal list") name=terminals ;;
  "terminal send") name=send ;;
  *) echo "unknown command" >&2; exit 2 ;;
esac
[ -f "$here/$name.json" ] && cat "$here/$name.json"
[ -f "$here/$name.err" ] && cat "$here/$name.err" >&2
exit "$(cat "$here/$name.code" 2>/dev/null || echo 0)"
"#;

const CLAUDE: &str = "term_aaaaaaaa-0000-4000-8000-000000000001";
const KIMI: &str = "term_bbbbbbbb-0000-4000-8000-000000000002";
const ASKING: &str = "term_cccccccc-0000-4000-8000-000000000003";
const SHELL: &str = "term_dddddddd-0000-4000-8000-000000000004";

fn ok(result: &str) -> String {
    format!(
        "{{\"id\": \"x\", \"ok\": true, \"result\": {result}, \"_meta\": {{\"runtimeId\": \"r\"}}}}"
    )
}

fn refused(code: &str) -> String {
    format!(
        "{{\"id\": \"x\", \"ok\": false, \"error\": {{\"code\": \"{code}\", \"message\": \"{code} Terminal prompt request ID: 1. Re-issue the exact command with --retry-request 1\", \"data\": {{}}}}, \"_meta\": {{}}}}"
    )
}

fn ps() -> String {
    ok(r#"{"worktrees": [
      {"path": "/code/api", "branch": "main", "agents": [
        {"paneKey": "tab-1:leaf-1", "state": "done", "agentType": "claude",
         "prompt": "Fix the flaky login test", "conversationName": "Fix login", "hasLivePty": true, "updatedAt": 200},
        {"paneKey": "tab-2:leaf-1", "state": "working", "agentType": "kimi", "prompt": "Port it",
         "conversationName": "Port the parser", "hasLivePty": true, "updatedAt": 300},
        {"paneKey": "tab-3:leaf-1", "state": "waiting", "agentType": "claude", "prompt": "Delete the cache",
         "conversationName": "Clean up", "hasLivePty": true, "updatedAt": 100}
      ]}
    ], "hostScope": {"hostIds": ["local"]}, "totalCount": 1, "truncated": false}"#)
}

fn terminals() -> String {
    let row = |handle: &str, tab: &str| {
        format!(
            "{{\"handle\": \"{handle}\", \"tabId\": \"{tab}\", \"leafId\": \"leaf-1\", \"worktreePath\": \"/code/api\", \"branch\": \"main\", \"title\": \"t\", \"connected\": true, \"writable\": true}}"
        )
    };
    ok(&format!(
        "{{\"terminals\": [{}, {}, {}, {}], \"totalCount\": 4, \"truncated\": false}}",
        row(CLAUDE, "tab-1"),
        row(KIMI, "tab-2"),
        row(ASKING, "tab-3"),
        row(SHELL, "tab-4"),
    ))
}

/// What Orca 1.4.258 printed for a prompt an agent took.
fn sent(handle: &str, stages: &str, provider: &str, warnings: &str) -> String {
    ok(&format!(
        "{{\"send\": {{\"handle\": \"{handle}\", \"accepted\": true, \"bytesWritten\": 44, \"prompt\": {{\"requestId\": \"q\", \"stages\": [{stages}], \"provider\": \"{provider}\", \"observation\": \"supported\"}}}}, \"mutation\": {{\"requestId\": \"q\", \"replayed\": false}}, \"warnings\": {warnings}}}"
    ))
}

struct Fake {
    dir: tempfile::TempDir,
}

impl Fake {
    /// A stand-in with three agent sessions open, and a plain shell.
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary folder");
        std::fs::write(dir.path().join("orca.sh"), ORCA_SH).expect("writes the stand-in");
        let fake = Fake { dir };
        fake.set("ps.json", &ps());
        fake.set("terminals.json", &terminals());
        fake
    }

    /// The stand-in runs as `/bin/sh <script>`: a script written a moment
    /// ago and run directly can fail as "text file busy" while tests run
    /// side by side.
    fn dispatcher(&self) -> Dispatcher {
        Dispatcher {
            tools: Tools {
                orca: Tool {
                    program: PathBuf::from("/bin/sh"),
                    args: vec![
                        self.dir
                            .path()
                            .join("orca.sh")
                            .to_string_lossy()
                            .into_owned(),
                    ],
                },
            },
            timing: Timing {
                command: Duration::from_secs(20),
                wait_submit: Duration::from_secs(5),
            },
        }
    }

    fn set(&self, name: &str, content: &str) {
        std::fs::write(self.dir.path().join(name), content).expect("writes what to print");
    }

    /// The arguments of each run, in order.
    fn calls(&self) -> Vec<Vec<String>> {
        let text = std::fs::read_to_string(self.dir.path().join("orca.calls")).unwrap_or_default();
        let mut calls: Vec<Vec<String>> = Vec::new();
        for line in text.lines() {
            if line == "--- call" {
                calls.push(Vec::new());
            } else if let (Some(call), Some(arg)) = (calls.last_mut(), line.strip_prefix("arg=")) {
                call.push(arg.replace('\u{1f}', "\n"));
            }
        }
        calls
    }

    /// The runs that typed something.
    fn sends(&self) -> Vec<Vec<String>> {
        self.calls()
            .into_iter()
            .filter(|call| call.get(1).map(String::as_str) == Some("send"))
            .collect()
    }
}

// ---- the list ---------------------------------------------------------------

#[test]
fn the_list_is_the_agent_sessions_open_in_orca_newest_first() {
    let fake = Fake::new();
    let listing = fake.dispatcher().sessions().expect("lists");

    assert_eq!(listing.host, "orca");
    assert!(listing.notes.is_empty());
    let seen: Vec<(&str, &str, State)> = listing
        .sessions
        .iter()
        .map(|s| (s.name.as_str(), s.agent.as_str(), s.state))
        .collect();
    assert_eq!(
        seen,
        [
            ("Port the parser", "kimi", State::Working),
            ("Fix login", "claude", State::Ready),
            ("Clean up", "claude", State::Asking),
        ],
        "the shell is not a session"
    );
    assert_eq!(listing.sessions[1].handle, CLAUDE);
    assert_eq!(listing.sessions[1].cwd, "/code/api");
    assert_eq!(
        listing.sessions[1].last_prompt.as_deref(),
        Some("Fix the flaky login test")
    );
    assert_eq!(
        fake.calls(),
        [["worktree", "ps", "--json"], ["terminal", "list", "--json"]]
    );
}

#[test]
fn a_list_orca_cut_short_says_so() {
    let fake = Fake::new();
    fake.set(
        "terminals.json",
        &terminals().replacen("\"truncated\": false", "\"truncated\": true", 1),
    );
    let listing = fake.dispatcher().sessions().expect("lists");
    assert!(
        listing.notes[0].contains("cut its list short"),
        "{:?}",
        listing.notes
    );
}

#[test]
fn without_orca_there_is_no_list_and_the_error_says_what_is_missing() {
    let mut dispatcher = Fake::new().dispatcher();
    dispatcher.tools.orca = Tool::named("/fastxt/no/such/orca");
    match dispatcher.sessions() {
        Err(Error::NotInstalled(name)) => assert_eq!(name, "orca"),
        other => panic!("expected NotInstalled, got {other:?}"),
    }
}

#[test]
fn an_error_orca_reports_is_told_in_its_words() {
    let fake = Fake::new();
    fake.set("ps.code", "1");
    fake.set(
        "ps.json",
        "{\"id\": \"x\", \"ok\": false, \"error\": {\"code\": \"runtime_unavailable\", \"message\": \"Orca is not running\"}}",
    );
    match fake.dispatcher().sessions() {
        Err(Error::Failed { message, .. }) => assert_eq!(message, "Orca is not running"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn output_that_is_not_orcas_is_unreadable_or_a_failure_never_an_empty_list() {
    let fake = Fake::new();
    fake.set("ps.json", "Usage: orca <command>\n");
    assert!(matches!(
        fake.dispatcher().sessions(),
        Err(Error::Unreadable { .. })
    ));
    fake.set("ps.code", "2");
    fake.set("ps.json", "");
    fake.set("ps.err", "orca: cannot reach the app\n");
    match fake.dispatcher().sessions() {
        Err(Error::Failed { message, .. }) => assert_eq!(message, "orca: cannot reach the app"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

// ---- sending ----------------------------------------------------------------

#[test]
fn a_prompt_is_typed_into_the_session_and_the_turn_is_seen_starting() {
    let fake = Fake::new();
    fake.set(
        "send.json",
        &sent(
            CLAUDE,
            "\"input_accepted\", \"turn_started\"",
            "claude",
            "null",
        ),
    );

    let receipt = fake
        .dispatcher()
        .send(CLAUDE, "  Now add a test for it\n")
        .expect("sends");

    assert_eq!(receipt.host, "orca");
    assert_eq!(receipt.handle, CLAUDE);
    assert_eq!(receipt.agent, "claude");
    assert_eq!(receipt.state_before, State::Ready);
    assert!(receipt.started);
    assert!(receipt.notes.is_empty());
    // The session is looked up again, then one command types and enters.
    assert_eq!(
        fake.calls(),
        [
            vec!["worktree", "ps", "--json"],
            vec!["terminal", "list", "--json"],
            vec![
                "terminal",
                "send",
                "--terminal",
                CLAUDE,
                "--text=Now add a test for it",
                "--enter",
                "--wait-submit",
                "5",
                "--json"
            ],
        ]
    );
}

#[test]
fn a_prompt_with_lines_and_a_leading_hyphen_is_still_one_argument() {
    let fake = Fake::new();
    fake.set(
        "send.json",
        &sent(CLAUDE, "\"input_accepted\"", "claude", "null"),
    );
    fake.dispatcher()
        .send(CLAUDE, "--verbose is noisy.\nTrim it, 请修复。")
        .expect("sends");
    assert_eq!(
        fake.sends()[0][4],
        "--text=--verbose is noisy.\nTrim it, 请修复。"
    );
}

#[test]
fn a_prompt_for_a_session_that_is_working_is_typed_and_not_seen_starting() {
    let fake = Fake::new();
    fake.set(
        "send.json",
        &sent(
            KIMI,
            "\"input_accepted\"",
            "kimi",
            "[\"input was accepted, but the turn was not seen starting.\"]",
        ),
    );
    let receipt = fake
        .dispatcher()
        .send(KIMI, "Then run the tests")
        .expect("sends");
    assert_eq!(receipt.state_before, State::Working);
    assert!(!receipt.started, "it waits behind what the agent is doing");
    assert_eq!(receipt.notes.len(), 1);
}

#[test]
fn nothing_is_typed_into_a_session_that_is_asking_a_question() {
    let fake = Fake::new();
    match fake.dispatcher().send(ASKING, "yes") {
        Err(Error::Asking(id)) => assert_eq!(id, "cccccccc"),
        other => panic!("expected Asking, got {other:?}"),
    }
    assert!(fake.sends().is_empty(), "the list said it was asking");
}

#[test]
fn a_question_that_came_up_after_the_list_is_caught_by_orca() {
    // The list still says ready; Orca sees the permission question and
    // types nothing. What it printed, to the letter.
    let fake = Fake::new();
    fake.set("send.code", "1");
    fake.set("send.json", &refused("agent_prompt_blocked"));
    match fake.dispatcher().send(CLAUDE, "Carry on") {
        Err(Error::Asking(id)) => assert_eq!(id, "aaaaaaaa"),
        other => panic!("expected Asking, got {other:?}"),
    }
}

#[test]
fn a_session_that_closed_since_the_list_is_gone_and_nothing_is_typed() {
    let fake = Fake::new();
    match fake
        .dispatcher()
        .send("term_eeeeeeee-0000-4000-8000-000000000005", "p")
    {
        Err(Error::Gone(id)) => assert_eq!(id, "eeeeeeee"),
        other => panic!("expected Gone, got {other:?}"),
    }
    assert!(fake.sends().is_empty());
}

#[test]
fn a_terminal_with_only_a_shell_in_it_is_never_typed_into() {
    // It is a live, writable terminal, and no agent runs in it: the prompt
    // would be run as a command.
    let fake = Fake::new();
    assert!(matches!(
        fake.dispatcher().send(SHELL, "rm -rf target"),
        Err(Error::Gone(_))
    ));
    assert!(fake.sends().is_empty());
}

#[test]
fn a_terminal_orca_can_no_longer_reach_is_gone() {
    for code in ["terminal_handle_stale", "terminal_not_writable"] {
        let fake = Fake::new();
        fake.set("send.code", "1");
        fake.set("send.json", &refused(code));
        assert!(
            matches!(fake.dispatcher().send(CLAUDE, "p"), Err(Error::Gone(_))),
            "{code}"
        );
    }
}

#[test]
fn a_prompt_that_reached_a_shell_after_all_is_an_alarm_not_a_receipt() {
    // The agent quit between the look and the send; Orca typed the text
    // into the shell and says no agent took it.
    let fake = Fake::new();
    fake.set(
        "send.json",
        &sent(
            CLAUDE,
            "\"input_accepted\"",
            "unsupported",
            "[\"input was accepted, but this provider cannot report delivery.\"]",
        ),
    );
    match fake.dispatcher().send(CLAUDE, "Carry on") {
        Err(Error::NotAnAgent(id)) => assert_eq!(id, "aaaaaaaa"),
        other => panic!("expected NotAnAgent, got {other:?}"),
    }
}

#[test]
fn input_orca_did_not_take_is_a_failure() {
    let fake = Fake::new();
    fake.set(
        "send.json",
        &ok("{\"send\": {\"handle\": \"h\", \"accepted\": false}, \"warnings\": null}"),
    );
    assert!(matches!(
        fake.dispatcher().send(CLAUDE, "p"),
        Err(Error::Failed { .. })
    ));
}

#[test]
fn any_other_refusal_is_told_in_orcas_words() {
    let fake = Fake::new();
    fake.set("send.code", "1");
    fake.set("send.json", &refused("host_busy"));
    match fake.dispatcher().send(CLAUDE, "p") {
        Err(Error::Failed { message, .. }) => assert!(message.starts_with("host_busy")),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_dry_run_checks_everything_and_types_nothing() {
    let fake = Fake::new();
    let plan = fake
        .dispatcher()
        .plan(CLAUDE, "Now add a test for it")
        .expect("plans");
    assert_eq!(plan.session.name, "Fix login");
    assert!(
        plan.command
            .args
            .contains(&"--text=Now add a test for it".to_string())
    );
    assert!(fake.sends().is_empty());
    // And it refuses what a send would.
    assert!(matches!(
        fake.dispatcher().plan(ASKING, "p"),
        Err(Error::Asking(_))
    ));
    assert!(matches!(
        fake.dispatcher().plan(SHELL, "p"),
        Err(Error::Gone(_))
    ));
}

#[test]
fn a_session_is_picked_from_the_list_by_the_start_of_its_handle() {
    let fake = Fake::new();
    let listing = fake.dispatcher().sessions().expect("lists");
    let session = resolve(&listing.sessions, "bbbb").expect("resolves");
    assert_eq!(session.handle, KIMI);
    assert!(matches!(
        resolve(&listing.sessions, "dddd"),
        Err(Error::NoSession(_))
    ));
}
