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

//! `fastxt sessions` and `fastxt send`: the coding-agent sessions open here,
//! or with `--on` on a paired computer, and typing a prompt into one.
//!
//! The sessions are the ones their user started and can see: agents running
//! in Orca's terminals. Nothing is typed into a session that is asking a
//! permission or trust question, nor into a terminal without an agent.

use super::{cut, home, note_key, open_db, print_json, tilde};
use fastxt::dispatch::{Dispatcher, wire};
use fastxt_core::model::{AgentSession, Sent};
use fastxt_core::sent::NewSent;
use fastxt_core::sync::client;
use serde::Serialize;
use std::io::{IsTerminal, Read};
use std::path::Path;

#[derive(clap::Args, Debug)]
pub struct SessionsArgs {
    /// The pairing code of another computer's `fastxt serve --agents`: list
    /// the sessions there instead of here.
    #[arg(long, value_name = "CODE")]
    pub on: Option<String>,

    /// Print JSON.
    #[arg(long)]
    pub json: bool,
}

#[derive(clap::Args, Debug)]
pub struct SendArgs {
    /// The note to send, by rowid or uuid. Sending it is recorded with it.
    #[arg(long, conflicts_with = "text")]
    pub note: Option<String>,

    /// Text to send instead of a note; with neither, the prompt is read from
    /// standard input. Only here: a paired computer is sent notes.
    // A prompt may well start with a hyphen; it is still the text.
    #[arg(long, allow_hyphen_values = true, conflicts_with = "on")]
    pub text: Option<String>,

    /// The session: its ID as `fastxt sessions` shows it, or the start of it.
    #[arg(long)]
    pub to: String,

    /// The pairing code of another computer's `fastxt serve --agents`: send
    /// to a session there instead of here.
    #[arg(long, value_name = "CODE")]
    pub on: Option<String>,

    /// Run every check and print the command; type nothing. Only here.
    #[arg(long, conflicts_with = "on")]
    pub dry_run: bool,

    /// Print JSON.
    #[arg(long)]
    pub json: bool,

    /// Database file for --note (default: the platform's Fastxt directory).
    #[arg(short, long)]
    pub db: Option<String>,
}

/// The sessions open here, as a paired device sees them, or there.
fn list(on: Option<&str>) -> Result<(Vec<AgentSession>, Vec<String>), String> {
    match on {
        Some(code) => client::agent_sessions(code)
            .map(|sessions| (sessions, Vec::new()))
            .map_err(|e| format!("cannot list the sessions on that computer: {e}")),
        None => Dispatcher::new()
            .sessions()
            .map(|listing| {
                (
                    listing.sessions.into_iter().map(wire).collect(),
                    listing.notes,
                )
            })
            .map_err(|e| format!("cannot list sessions: {e}")),
    }
}

#[derive(Serialize)]
struct SessionsOut<'a> {
    sessions: &'a [AgentSession],
    notes: &'a [String],
}

pub fn sessions(args: &SessionsArgs) -> Result<(), String> {
    let (sessions, notes) = list(args.on.as_deref())?;
    if args.json {
        return print_json(&SessionsOut {
            sessions: &sessions,
            notes: &notes,
        });
    }
    // Paths from another computer are that computer's: no `~` for them.
    let home = if args.on.is_some() { None } else { home() };
    for line in listing_lines(&sessions, &notes, args.on.is_some(), home.as_deref()) {
        println!("{line}");
    }
    Ok(())
}

/// The first eight characters of a handle, after the host's prefix.
fn short(handle: &str) -> &str {
    let id = handle.strip_prefix("term_").unwrap_or(handle);
    id.char_indices().nth(8).map_or(id, |(at, _)| &id[..at])
}

fn session_line(session: &AgentSession, home: Option<&Path>) -> String {
    let mut place = tilde(Path::new(&session.cwd), home);
    if let Some(branch) = &session.branch {
        place = format!("{place} · {branch}");
    }
    format!(
        "  {:<8}  {:<11}  {:<7}  {:<36}  {place}",
        short(&session.handle),
        cut(&session.agent, 11),
        session.state,
        cut(&session.name, 36),
    )
    .trim_end()
    .to_string()
}

fn listing_lines(
    sessions: &[AgentSession],
    notes: &[String],
    there: bool,
    home: Option<&Path>,
) -> Vec<String> {
    let place = if there { "on that computer" } else { "here" };
    let mut lines = Vec::new();
    if sessions.is_empty() {
        lines.push(format!("No agent session is open {place}."));
    } else {
        lines.push(format!(
            "Agent sessions open {place}, the one that changed last first"
        ));
        lines.extend(sessions.iter().map(|s| session_line(s, home)));
    }
    lines.extend(notes.iter().map(|note| format!("{note}.")));
    lines
}

/// The session a handle, or the start of one, names.
fn pick<'a>(sessions: &'a [AgentSession], wanted: &str) -> Result<&'a AgentSession, String> {
    let wanted = wanted.trim();
    if let Some(exact) = sessions.iter().find(|s| s.handle == wanted) {
        return Ok(exact);
    }
    let starts: Vec<&AgentSession> = sessions
        .iter()
        .filter(|s| {
            !wanted.is_empty()
                && (s.handle.starts_with(wanted)
                    || s.handle
                        .strip_prefix("term_")
                        .is_some_and(|id| id.starts_with(wanted)))
        })
        .collect();
    match starts.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "no open session matches {wanted:?}; `fastxt sessions` lists them"
        )),
        many => Err(format!(
            "{wanted:?} matches {} open sessions; give more of it",
            many.len()
        )),
    }
}

/// What `fastxt send --json` prints.
#[derive(Serialize)]
struct SendOut<'a> {
    session: &'a AgentSession,
    /// Whether the agent was seen starting on it.
    started: bool,
    /// The record of it, when a note was sent.
    sent: Option<&'a Sent>,
    notes: &'a [String],
}

/// The note's id and text, for --note.
fn note_text(key: &str, db: &fastxt_core::Fastxt) -> Result<(String, String), String> {
    let key = note_key(key);
    match db.get(&key) {
        Ok(Some(note)) => Ok((note.uuid4, note.txt)),
        Ok(None) => Err(format!("there is no note with {key}")),
        Err(e) => Err(format!("cannot read the note: {e}")),
    }
}

fn stdin_text() -> Result<String, String> {
    if std::io::stdin().is_terminal() {
        return Err("nothing to send: give --note or --text, or pipe the prompt in".to_string());
    }
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| format!("cannot read the prompt from standard input: {e}"))?;
    Ok(text)
}

pub fn send(args: &SendArgs) -> Result<(), String> {
    match &args.on {
        Some(code) => send_there(args, code),
        None => send_here(args),
    }
}

fn send_there(args: &SendArgs, code: &str) -> Result<(), String> {
    let Some(note) = &args.note else {
        return Err("with --on, give --note: a paired computer is sent notes".to_string());
    };
    let mut db = open_db(args.db.as_deref())?;
    let (sessions, _) = list(Some(code))?;
    let session = pick(&sessions, &args.to)?.clone();
    let sent = client::send_note(code, &mut db, &note_key(note), &session.handle)
        .map_err(|e| format!("not sent: {e}"))?;
    if args.json {
        return print_json(&SendOut {
            session: &session,
            started: sent.started,
            sent: Some(&sent),
            notes: &[],
        });
    }
    println!("{}", receipt_line(&session, sent.started));
    Ok(())
}

fn send_here(args: &SendArgs) -> Result<(), String> {
    // A note is read, and its sending recorded, in the database.
    let mut db = args
        .note
        .as_ref()
        .map(|_| open_db(args.db.as_deref()))
        .transpose()?;
    let (note_uuid, prompt) = match (&args.note, &mut db) {
        (Some(note), Some(db)) => {
            let (uuid, text) = note_text(note, db)?;
            (Some(uuid), text)
        }
        _ => match &args.text {
            Some(text) => (None, text.clone()),
            None => (None, stdin_text()?),
        },
    };
    let dispatcher = Dispatcher::new();
    let (sessions, _) = list(None)?;
    let session = pick(&sessions, &args.to)?.clone();

    if args.dry_run {
        let plan = dispatcher
            .plan(&session.handle, &prompt)
            .map_err(|e| format!("cannot send: {e}"))?;
        if args.json {
            return print_json(&serde_json::json!({
                "dry_run": true,
                "session": session,
                "command": plan.command.line(usize::MAX),
            }));
        }
        println!(
            "Would type it into {} session {} ({}), with:",
            session.agent,
            short(&session.handle),
            session.name
        );
        println!("  {}", plan.command.line(60));
        println!("Nothing was typed.");
        return Ok(());
    }

    let receipt = dispatcher
        .send(&session.handle, &prompt)
        .map_err(|e| format!("not sent: {e}"))?;
    let sent = match (note_uuid, &mut db) {
        (Some(note_uuid), Some(db)) => Some(
            db.record_sent(NewSent {
                note_uuid,
                host: receipt.host.clone(),
                agent: receipt.agent.clone(),
                session: receipt.name.clone(),
                cwd: receipt.cwd.clone(),
                started: receipt.started,
            })
            .map_err(|e| format!("it was typed, but not recorded: {e}"))?,
        ),
        _ => None,
    };
    if args.json {
        return print_json(&SendOut {
            session: &session,
            started: receipt.started,
            sent: sent.as_ref(),
            notes: &receipt.notes,
        });
    }
    println!("{}", receipt_line(&session, receipt.started));
    for note in &receipt.notes {
        println!("  Orca said: {note}");
    }
    Ok(())
}

fn receipt_line(session: &AgentSession, started: bool) -> String {
    let id = short(&session.handle);
    let agent = &session.agent;
    let name = &session.name;
    if started {
        format!("Typed into {agent} session {id} ({name}); it started on it.")
    } else if session.state == "working" {
        format!(
            "Typed into {agent} session {id} ({name}). It was working, so the prompt waits behind what it is doing."
        )
    } else {
        format!(
            "Typed into {agent} session {id} ({name}). It was not seen starting on it yet; look at the session."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(handle: &str, state: &str) -> AgentSession {
        AgentSession {
            host: "orca".into(),
            handle: handle.to_string(),
            agent: "claude".into(),
            name: "Fix login".into(),
            cwd: "/Users/me/code/api".into(),
            branch: Some("main".into()),
            state: state.to_string(),
            ..AgentSession::default()
        }
    }

    #[test]
    fn a_session_is_picked_by_its_handle_or_the_start_of_it() {
        let sessions = [
            session("term_5e47b923-0963-451d-ba71-d0756e7ba2cc", "ready"),
            session("term_5e47ffff-0000-4000-8000-000000000000", "ready"),
            session("term_aaaaaaaa-0000-4000-8000-000000000001", "ready"),
        ];
        let found = |wanted| pick(&sessions, wanted).map(|s| short(&s.handle).to_string());
        assert_eq!(found("aaaa").unwrap(), "aaaaaaaa");
        assert_eq!(found(" term_aaaa ").unwrap(), "aaaaaaaa");
        assert_eq!(
            found("term_5e47b923-0963-451d-ba71-d0756e7ba2cc").unwrap(),
            "5e47b923"
        );
        assert!(found("5e47").unwrap_err().contains("matches 2"));
        assert!(found("ffff").unwrap_err().contains("no open session"));
        assert!(found("").is_err());
    }

    #[test]
    fn the_list_reads_as_lines_with_home_as_a_tilde_here_only() {
        let sessions = [session("term_5e47b923-0963", "ready")];
        let here = listing_lines(&sessions, &[], false, Some(Path::new("/Users/me")));
        assert_eq!(
            here[0],
            "Agent sessions open here, the one that changed last first"
        );
        assert_eq!(
            here[1],
            "  5e47b923  claude       ready    Fix login                             ~/code/api · main"
        );
        let there = listing_lines(&sessions, &[], true, None);
        assert!(there[0].contains("on that computer"));
        assert!(there[1].ends_with("/Users/me/code/api · main"));
        let none = listing_lines(&[], &["Orca cut its list short".into()], false, None);
        assert_eq!(
            none,
            ["No agent session is open here.", "Orca cut its list short."]
        );
    }

    #[test]
    fn a_receipt_says_whether_the_agent_started_on_it() {
        let ready = session("term_5e47b923", "ready");
        assert_eq!(
            receipt_line(&ready, true),
            "Typed into claude session 5e47b923 (Fix login); it started on it."
        );
        assert!(receipt_line(&ready, false).contains("not seen starting"));
        assert!(receipt_line(&session("term_5e47b923", "working"), false).contains("waits behind"));
    }

    #[test]
    fn a_note_is_found_by_rowid_or_uuid_and_a_missing_one_says_so() {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let mut db = fastxt_core::Fastxt::open(dir.path().join("notes.sqlite3")).expect("opens");
        let note = db
            .insert(fastxt_core::model::NewNote::new(
                "Fix the flaky login test",
                "prompt",
            ))
            .expect("inserts");
        let (uuid, text) = note_text(&note.rowid.to_string(), &db).expect("by rowid");
        assert_eq!(
            (uuid.as_str(), text.as_str()),
            (note.uuid4.as_str(), "Fix the flaky login test")
        );
        assert_eq!(
            note_text(&note.uuid4, &db).expect("by uuid").1,
            "Fix the flaky login test"
        );
        let missing = note_text("999", &db).expect_err("no such note");
        assert!(missing.contains("no note with rowid 999"), "{missing}");
    }

    #[test]
    fn a_paired_computer_is_sent_notes_and_only_notes() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrap {
            #[command(flatten)]
            send: SendArgs,
        }
        let code = "FASTXT1:192.168.1.5:3456:ffffffffffffffffffffffffffffffff:AAAAAAAAAA";
        assert!(Wrap::try_parse_from(["x", "--note", "7", "--to", "5e47", "--on", code]).is_ok());
        assert!(Wrap::try_parse_from(["x", "--text", "p", "--to", "5e47", "--on", code]).is_err());
        assert!(
            Wrap::try_parse_from([
                "x",
                "--note",
                "7",
                "--to",
                "5e47",
                "--on",
                code,
                "--dry-run"
            ])
            .is_err()
        );
        assert!(Wrap::try_parse_from(["x", "--note", "1", "--text", "p", "--to", "5e47"]).is_err());
        let hyphen = Wrap::try_parse_from(["x", "--text", "--verbose is noisy", "--to", "5e47"])
            .expect("parses");
        assert_eq!(hyphen.send.text.as_deref(), Some("--verbose is noisy"));
        // Without --note, sending there is refused before any network.
        let no_note = SendArgs {
            note: None,
            text: None,
            to: "5e47".into(),
            on: Some(code.into()),
            dry_run: false,
            json: false,
            db: None,
        };
        assert!(send(&no_note).unwrap_err().contains("give --note"));
    }
}
