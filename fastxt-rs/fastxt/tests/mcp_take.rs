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

//! `fastxt mcp` end to end: a client speaks MCP to the real command over its
//! standard input and output, as an agent does, against a temporary
//! database. It lists the prompts waiting there and takes them, with the
//! tool and as the `take` prompt, and each take is kept with the client's
//! name and the folder the server ran in.

use fastxt_core::Fastxt;
use fastxt_core::model::{NewNote, Sent};
use fastxt_core::sent::NewSent;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

/// An MCP client on the other end of `fastxt mcp`.
struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

impl Client {
    fn start(db: &Path, cwd: &Path) -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fastxt"))
            .arg("mcp")
            .arg("--db")
            .arg(db)
            .current_dir(cwd)
            .env_remove("FASTXT_DB")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("fastxt mcp starts");
        let stdout = child.stdout.take().expect("its output");
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Client {
            child,
            stdin,
            lines,
            next_id: 0,
        }
    }

    fn write(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().expect("still open");
        writeln!(stdin, "{message}").expect("the server reads");
        stdin.flush().expect("flushed");
    }

    /// Send a request and wait for the response to it.
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.write(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(30))
                .expect("an answer within 30 s");
            let message: Value = serde_json::from_str(&line).expect("each line is JSON");
            if message["id"] == id {
                return message;
            }
        }
    }

    /// Call a tool that answers; its text, read as JSON.
    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let response = self.request("tools/call", json!({"name": name, "arguments": arguments}));
        let result = &response["result"];
        assert_ne!(result["isError"], true, "{response}");
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("no text in {response}"));
        serde_json::from_str(text).expect("the tool answers JSON")
    }

    /// Hang up, as an agent does when it quits; the server ends cleanly.
    fn hang_up(mut self) {
        drop(self.stdin.take());
        for _ in 0..100 {
            if let Some(status) = self.child.try_wait().expect("a status") {
                assert!(status.success(), "fastxt mcp ended with {status}");
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("fastxt mcp kept running after its client hung up");
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn an_agent_lists_the_waiting_prompts_and_takes_them() {
    let dir = tempfile::tempdir().expect("a temporary folder");
    let db_path = dir.path().join("notes.sqlite3");
    let project = dir.path().join("api");
    std::fs::create_dir(&project).expect("a project folder");
    let milk = {
        let mut db = Fastxt::open(&db_path).expect("opens");
        db.insert(NewNote::new("Cache the session list", "kind:idea"))
            .unwrap();
        db.insert(NewNote::new("Fix the flaky login test", "kind:prompt,auth"))
            .unwrap();
        let milk = db.insert(NewNote::new("Buy milk", "")).unwrap();
        let sent = db
            .insert(NewNote::new("Rename the crate", "kind:prompt"))
            .unwrap();
        db.record_sent(NewSent {
            note_uuid: sent.uuid4,
            host: "orca".into(),
            agent: "claude".into(),
            session: "Rename".into(),
            cwd: "/code/api".into(),
            started: true,
        })
        .unwrap();
        db.insert(NewNote::new("Add a retry to the upload", "kind:prompt"))
            .unwrap();
        milk
    };

    let mut client = Client::start(&db_path, &project);
    let init = client.request(
        "initialize",
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {"name": "test-agent", "title": "Test Agent", "version": "1.0"},
        }),
    );
    let capabilities = &init["result"]["capabilities"];
    assert!(
        capabilities["tools"].is_object() && capabilities["prompts"].is_object(),
        "{init}"
    );
    client.write(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));

    let tools = client.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    assert!(
        names.contains(&"waiting_prompts") && names.contains(&"take_prompt"),
        "{names:?}"
    );

    // Waiting: the two prompts nothing has had, newest first. Not the idea,
    // the plain note, nor the prompt already typed into a session.
    let waiting = client.tool("waiting_prompts", json!({}));
    assert_eq!(waiting["count"], 2);
    let texts: Vec<&str> = waiting["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|prompt| prompt["text"].as_str())
        .collect();
    assert_eq!(
        texts,
        ["Add a retry to the upload", "Fix the flaky login test"]
    );
    assert_eq!(
        waiting["prompts"][1]["tags"],
        json!(["auth"]),
        "the kind is not shown as a tag"
    );
    let ideas = client.tool("waiting_prompts", json!({"kind": "idea"}));
    assert_eq!(ideas["prompts"][0]["text"], "Cache the session list");

    // The `take` prompt, with words: the login prompt, as the user's message.
    let prompts = client.request("prompts/list", json!({}));
    let take = &prompts["result"]["prompts"][0];
    assert_eq!(take["name"], "take", "{prompts}");
    assert_eq!(take["arguments"][0]["name"], "note");
    assert_eq!(take["arguments"][0]["required"], false);
    let got = client.request(
        "prompts/get",
        json!({"name": "take", "arguments": {"note": "login"}}),
    );
    let message = &got["result"]["messages"][0];
    assert_eq!(message["role"], "user", "{got}");
    assert_eq!(message["content"]["text"], "Fix the flaky login test");

    // The tool, with nothing named: the newest prompt still waiting.
    let taken = client.tool("take_prompt", json!({}));
    assert_eq!(taken["text"], "Add a retry to the upload");
    assert_eq!(taken["taken"]["host"], Sent::TAKEN);
    assert_eq!(taken["taken"]["agent"], "Test Agent");

    // Nothing waits now. The tool says so and how prompts arrive; the
    // prompt is refused rather than sent as an empty message.
    let none = client.tool("take_prompt", json!({}));
    assert_eq!(none["text"], Value::Null);
    assert!(
        none["message"].as_str().unwrap().contains("Sync notes now"),
        "{none}"
    );
    let empty = client.tool("waiting_prompts", json!({}));
    assert_eq!(empty["count"], 0);
    assert!(empty["message"].is_string(), "{empty}");
    let refused = client.request("prompts/get", json!({"name": "take"}));
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("No prompt is waiting")),
        "{refused}"
    );

    // Everything, sent or taken, with where it went.
    let all = client.tool("waiting_prompts", json!({"include_sent": true}));
    assert_eq!(all["count"], 3);
    let rename = all["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|prompt| prompt["text"] == "Rename the crate")
        .expect("listed");
    assert_eq!(rename["sent"][0]["host"], "orca");

    // A note given by its number is taken whatever its kind; a missing one
    // is an error, not an empty message.
    let by_number = client.request(
        "prompts/get",
        json!({"name": "take", "arguments": {"note": milk.rowid.to_string()}}),
    );
    assert_eq!(
        by_number["result"]["messages"][0]["content"]["text"],
        "Buy milk"
    );
    let missing = client.request(
        "tools/call",
        json!({"name": "take_prompt", "arguments": {"rowid": 999}}),
    );
    assert!(
        missing.to_string().contains("no note with rowid 999"),
        "{missing}"
    );

    client.hang_up();

    // The takes are kept, with who took each and the folder it worked in.
    let db = Fastxt::open(&db_path).expect("opens");
    let takes: Vec<Sent> = db
        .sent(None, 10)
        .unwrap()
        .into_iter()
        .filter(Sent::taken)
        .collect();
    assert_eq!(takes.len(), 3, "{takes:?}");
    let project = project.canonicalize().unwrap();
    for take in &takes {
        assert_eq!(take.agent, "Test Agent");
        assert_eq!(Path::new(&take.cwd).canonicalize().unwrap(), project);
        assert_eq!(take.session, "");
        assert!(take.started);
    }
}
