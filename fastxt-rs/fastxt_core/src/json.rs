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

//! The JSON command protocol, used by the mobile apps over the FFI.
//!
//! Rust clients (desktop, MCP, CLI) call the typed API directly. Every
//! response here is produced by serde from typed values — no hand-built JSON
//! strings — and every failure is `{"error": "…"}`.
//!
//! ```text
//! {"action": "search", "query": "rust", "limit": 20, "offset": 0}
//! ```

use crate::ai;
use crate::error::{Error, Result};
use crate::model::{NoteKey, Page, ScoredNote, Settings, TagCount};
use crate::store::{Fastxt, SharedDb};
use crate::sync;
use serde::Serialize;
use serde_json::{Value, json};
use std::sync::Mutex;

/// Run one JSON command against `db`. Never panics on bad input.
#[must_use]
pub fn run(db: &SharedDb, input: &str) -> String {
    let request: Value = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => return error_json("the command is not valid JSON"),
    };
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard = match db.lock() {
            Ok(g) => g,
            Err(_) => return error_json("the database is locked by another operation"),
        };
        dispatch(&mut guard, &action, &request)
    }))
    .unwrap_or_else(|_| error_json("the command panicked; this is a bug"))
}

fn error_json(message: impl std::fmt::Display) -> String {
    serde_json::to_string(&json!({ "error": message.to_string() }))
        .unwrap_or_else(|_| r#"{"error":"response failed to serialize"}"#.into())
}

fn err(error: Error) -> String {
    error_json(error)
}

fn ok<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| error_json("serialization failed"))
}

fn key_of(request: &Value) -> Result<NoteKey> {
    if let Some(rowid) = request.get("rowid").and_then(Value::as_i64) {
        return Ok(NoteKey::Rowid(rowid));
    }
    if let Some(uuid) = request.get("uuid4").and_then(Value::as_str) {
        return Ok(NoteKey::Uuid(uuid.to_string()));
    }
    Err(Error::Invalid("the command needs a rowid or uuid4".into()))
}

fn page_json(page: &Page) -> String {
    ok(&json!({ "count": page.count, "notes": page.notes }))
}

fn dispatch(db: &mut Fastxt, action: &str, request: &Value) -> String {
    match action {
        // ---- notes ------------------------------------------------------
        "select" => {
            let limit = u32(request, "limit").unwrap_or(20);
            let offset = u32(request, "offset").unwrap_or(0);
            let category = request.get("category").and_then(Value::as_str);
            let filter = crate::model::Filter {
                category: category.map(str::to_string),
            };
            match db.list(limit, offset, &filter) {
                Ok(page) => page_json(&page),
                Err(e) => err(e),
            }
        }
        "search" => {
            let query = str(request, "query");
            let limit = u32(request, "limit").unwrap_or(20);
            let offset = u32(request, "offset").unwrap_or(0);
            match db.search(&query, limit, offset) {
                Ok(page) => page_json(&page),
                Err(e) => err(e),
            }
        }
        "get" => match key_of(request).and_then(|k| db.get(&k)) {
            Ok(Some(note)) => ok(&json!({ "note": note })),
            Ok(None) => err(Error::NotFound("no such note".into())),
            Err(e) => err(e),
        },
        "insert" => {
            let new = crate::model::NewNote {
                txt: str(request, "txt"),
                tags: str(request, "tags"),
                ai_tags: request.get("ai_tags").and_then(Value::as_array).map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                }),
                ai_summary: opt_str(request, "ai_summary"),
                ai_category: opt_str(request, "ai_category"),
            };
            match db.insert(new) {
                Ok(note) => ok(&json!({ "note": note })),
                Err(e) => err(e),
            }
        }
        "update" => {
            let key = match key_of(request) {
                Ok(k) => k,
                Err(e) => return err(e),
            };
            match db.update(&key, &str(request, "txt"), &str(request, "tags")) {
                Ok(note) => ok(&json!({ "note": note })),
                Err(e) => err(e),
            }
        }
        "delete" => match key_of(request).and_then(|k| db.delete(&k)) {
            Ok(deleted) => ok(&json!({ "deleted": deleted })),
            Err(e) => err(e),
        },

        // ---- categories / tags -------------------------------------------
        "categories" => match db.categories() {
            Ok(categories) => ok(&json!({ "categories": categories })),
            Err(e) => err(e),
        },
        "rename-category" => {
            let old = str(request, "old");
            let new = str(request, "new");
            match db.rename_category(&old, &new) {
                Ok(updated) => ok(&json!({ "updated": updated })),
                Err(e) => err(e),
            }
        }
        "dismiss-category" => match db.dismiss_category(&str(request, "category")) {
            Ok(updated) => ok(&json!({ "updated": updated })),
            Err(e) => err(e),
        },
        "tag-vocabulary" => {
            let limit = usize(request, "limit").unwrap_or(50);
            match db.tag_vocabulary(limit) {
                Ok(tags) => ok(&json!({ "tags": tags })),
                Err(e) => err(e),
            }
        }

        // ---- AI ----------------------------------------------------------
        "ai-check" => {
            let check = match ai::Ai::for_db(db) {
                Ok(ai) => ai.check(),
                Err(e) => return err(e),
            };
            ok(&json!({ "ok": check.ok, "message": check.message, "backend": ai_backend_name(db) }))
        }
        "ai-tag" => {
            let text = str(request, "text");
            let vocabulary: Vec<String> = db
                .tag_vocabulary(40)
                .unwrap_or_default()
                .into_iter()
                .map(|t: TagCount| t.tag)
                .collect();
            let result = ai::Ai::for_db(db).and_then(|ai| ai.suggest_tags(&text, &vocabulary));
            match result {
                Ok(tags) => ok(&json!({ "tags": tags, "available": true })),
                Err(e) => ok(&json!({ "tags": [], "available": false, "error": e.to_string() })),
            }
        }
        "ai-summarize" => {
            // Summarizes the given text; it does NOT save anything.
            let result = ai::summarize_text(db, &str(request, "text"));
            match result {
                Ok(summary) => ok(&json!({ "summary": summary, "available": true })),
                Err(e) => {
                    ok(&json!({ "summary": null, "available": false, "error": e.to_string() }))
                }
            }
        }
        "ai-tag-all" => {
            let limit = u32(request, "limit").unwrap_or(100);
            match run_job(db, limit, ai::tag_notes) {
                Ok(report) => ok(&report),
                Err(e) => ok(&json!({ "processed": 0, "errors": 0, "error": e.to_string() })),
            }
        }
        "ai-embed-all" => {
            let limit = u32(request, "limit").unwrap_or(100);
            match run_job(db, limit, ai::embed_notes) {
                Ok(report) => ok(&report),
                Err(e) => ok(&json!({ "processed": 0, "errors": 0, "error": e.to_string() })),
            }
        }
        "ai-organize" => {
            let limit = u32(request, "limit").unwrap_or(100);
            match run_job(db, limit, ai::categorize_notes) {
                Ok(report) => ok(&report),
                Err(e) => ok(&json!({ "processed": 0, "errors": 0, "error": e.to_string() })),
            }
        }
        "semantic-search" => {
            let query = str(request, "query");
            let limit = u32(request, "limit").unwrap_or(10);
            match semantic(db, &query, limit) {
                Ok(results) => ok(&json!({ "results": results, "available": true })),
                Err(e) => ok(&json!({ "results": [], "available": false, "error": e.to_string() })),
            }
        }
        "hybrid-search" => {
            let query = str(request, "query");
            let limit = u32(request, "limit").unwrap_or(10);
            match ai::hybrid_with_embeddings(db, &query, limit) {
                Ok(results) => ok(&json!({ "results": results, "used_embedding": true })),
                Err(Error::AiUnavailable(message)) => {
                    // AI down: still answer from the text index.
                    match db.hybrid_search(&query, None, limit) {
                        Ok(results) => ok(&json!({
                            "results": results,
                            "used_embedding": false,
                            "error": message,
                        })),
                        Err(e) => err(e),
                    }
                }
                Err(e) => err(e),
            }
        }
        "embedding-models" => match db.embedding_models() {
            Ok(models) => ok(&json!({ "models": models })),
            Err(e) => err(e),
        },

        // ---- settings ------------------------------------------------------
        "settings-get" => match db.settings() {
            Ok(settings) => ok(&json!({ "settings": settings })),
            Err(e) => err(e),
        },
        "settings-set" => {
            let Some(raw) = request.get("settings") else {
                return err(Error::Invalid(
                    "settings-set needs a settings object".into(),
                ));
            };
            match serde_json::from_value::<Settings>(raw.clone()) {
                Ok(settings) => match db.save_settings(&settings) {
                    Ok(()) => ok(&json!({ "ok": true })),
                    Err(e) => err(e),
                },
                Err(e) => err(Error::Invalid(format!("bad settings: {e}"))),
            }
        }

        // ---- sync ----------------------------------------------------------
        "sync" => {
            let code = str(request, "code");
            match sync::client::sync(&code, db) {
                Ok(report) => ok(&json!({ "report": report })),
                Err(e) => err(e),
            }
        }
        "server-start" => {
            let port = u16(request, "port").unwrap_or(sync::DEFAULT_PORT);
            // The global server needs its own connection to the same file.
            let path = match db.path() {
                Some(p) => p.to_path_buf(),
                None => {
                    return err(Error::Sync(
                        "the sync server needs a file-backed database".into(),
                    ));
                }
            };
            let handle = match Fastxt::open(path) {
                Ok(server_db) => {
                    let shared: SharedDb = std::sync::Arc::new(Mutex::new(server_db));
                    match sync::server::start_global(shared, port) {
                        Ok(h) => h,
                        Err(e) => return err(e),
                    }
                }
                Err(e) => return err(e),
            };
            ok(&json!({ "pairing_code": handle.pairing_code, "addr": handle.addr().to_string() }))
        }
        "server-stop" => {
            sync::server::stop_global();
            ok(&json!({ "ok": true }))
        }
        "server-status" => match sync::server::global() {
            Some(handle) => ok(&json!({
                "running": true,
                "pairing_code": handle.pairing_code,
                "addr": handle.addr().to_string(),
            })),
            None => ok(&json!({ "running": false })),
        },
        "db-info" => match (db.path(), db.schema_version(), db.device_id()) {
            (path, Ok(version), device_id) => ok(&json!({
                "path": path.map(|p| p.display().to_string()),
                "schema_version": version,
                "device_id": device_id,
            })),
            (path, Err(e), _) => {
                let _ = path;
                err(e)
            }
        },

        _ => error_json(format!("unknown action: {action:?}")),
    }
}

fn ai_backend_name(db: &Fastxt) -> String {
    db.settings()
        .map(|s| s.ai.backend)
        .unwrap_or_else(|_| "unknown".into())
}

type Job = fn(&mut Fastxt, u32, &dyn Fn(u32, u32)) -> Result<ai::JobReport>;

fn run_job(db: &mut Fastxt, limit: u32, job: Job) -> Result<serde_json::Value> {
    let silent = |_, _: u32| {};
    let report = job(db, limit, &silent)?;
    Ok(json!({
        "processed": report.processed,
        "errors": report.errors,
    }))
}

fn semantic(db: &mut Fastxt, query: &str, limit: u32) -> Result<Vec<ScoredNote>> {
    let ai = ai::Ai::for_db(db)?;
    let check = ai.check();
    if !check.ok {
        return Err(Error::AiUnavailable(check.message));
    }
    let vector = ai.embed(query)?;
    db.semantic_search(&vector, ai.embedding_model_id(), limit, 0.0)
}

// ---- tiny accessors --------------------------------------------------------

fn str(request: &Value, field: &str) -> String {
    request
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or("")
        .into()
}

fn opt_str(request: &Value, field: &str) -> Option<String> {
    request
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
}

macro_rules! number {
    ($name:ident, $t:ty) => {
        fn $name(request: &Value, field: &str) -> Option<$t> {
            match request.get(field) {
                Some(Value::Number(n)) => n.as_i64().and_then(|v| <$t>::try_from(v).ok()),
                Some(Value::String(s)) => s.parse().ok(),
                _ => None,
            }
        }
    };
}
number!(u32, u32);
number!(u16, u16);
number!(usize, usize);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn db() -> SharedDb {
        Arc::new(Mutex::new(Fastxt::open_in_memory().unwrap()))
    }

    fn run_json(shared: &SharedDb, cmd: Value) -> Value {
        serde_json::from_str(&run(shared, &cmd.to_string())).unwrap()
    }

    #[test]
    fn insert_returns_the_created_note_and_get_finds_it() {
        let shared = db();
        let created = run_json(
            &shared,
            json!({"action": "insert", "txt": "hello", "tags": "a, b"}),
        );
        let note = created.get("note").unwrap();
        assert!(note.get("rowid").and_then(Value::as_i64).unwrap() > 0);
        assert_eq!(note["tags"], "a,b");
        let uuid = note["uuid4"].as_str().unwrap().to_string();

        let got = run_json(&shared, json!({"action": "get", "uuid4": uuid}));
        assert_eq!(got["note"]["txt"], "hello");
    }

    #[test]
    fn update_keeps_the_same_rowid_and_delete_tombstones() {
        let shared = db();
        let created = run_json(
            &shared,
            json!({"action": "insert", "txt": "v1", "tags": ""}),
        );
        let rowid = created["note"]["rowid"].as_i64().unwrap();
        let updated = run_json(
            &shared,
            json!({"action": "update", "rowid": rowid, "txt": "v2", "tags": "x"}),
        );
        assert_eq!(updated["note"]["rowid"], rowid);
        assert_eq!(updated["note"]["txt"], "v2");

        run_json(&shared, json!({"action": "delete", "rowid": rowid}));
        let missing = run_json(&shared, json!({"action": "get", "rowid": rowid}));
        assert!(missing.get("error").is_some());
        let page = run_json(
            &shared,
            json!({"action": "select", "limit": 10, "offset": 0}),
        );
        assert_eq!(page["count"], 0);
    }

    #[test]
    fn search_and_categories_flow() {
        let shared = db();
        run_json(
            &shared,
            json!({"action": "insert", "txt": "rust note", "tags": "code"}),
        );
        run_json(
            &shared,
            json!({"action": "insert", "txt": "soup recipe", "tags": "food"}),
        );
        let found = run_json(&shared, json!({"action": "search", "query": "rust"}));
        assert_eq!(found["count"], 1);
        assert_eq!(found["notes"][0]["txt"], "rust note");

        let categories = run_json(&shared, json!({"action": "categories"}));
        assert_eq!(categories["categories"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn bad_input_produces_error_json_not_panics() {
        let shared = db();
        assert_eq!(
            run(&shared, "not json"),
            r#"{"error":"the command is not valid JSON"}"#
        );
        let unknown = run_json(&shared, json!({"action": "does-not-exist"}));
        assert!(
            unknown["error"]
                .as_str()
                .unwrap()
                .contains("unknown action")
        );
        let missing = run_json(&shared, json!({"action": "get"}));
        assert!(missing.get("error").is_some());
        // insert with a number where a string belongs is an error, not a crash
        let weird = run_json(&shared, json!({"action": "insert", "txt": 42, "tags": ""}));
        assert!(weird.get("error").is_some());
    }

    #[test]
    fn settings_round_trip_through_json() {
        let shared = db();
        let set = run_json(
            &shared,
            json!({"action": "settings-set", "settings": {
                "ai": {"backend": "llamacpp", "embedding_model": "bge-m3"}
            }}),
        );
        assert_eq!(set["ok"], true);
        let got = run_json(&shared, json!({"action": "settings-get"}));
        assert_eq!(got["settings"]["ai"]["backend"], "llamacpp");
        assert_eq!(got["settings"]["ai"]["embedding_model"], "bge-m3");
    }

    #[test]
    fn hybrid_search_degrades_to_text_when_ai_is_down() {
        let shared = db();
        run_json(
            &shared,
            json!({"action": "insert", "txt": "databases note", "tags": ""}),
        );
        run_json(
            &shared,
            json!({"action": "settings-set", "settings": {
                "ai": {"endpoint": "http://127.0.0.1:1"}
            }}),
        );
        let out = run_json(
            &shared,
            json!({"action": "hybrid-search", "query": "databases"}),
        );
        assert_eq!(out["used_embedding"], false);
        assert_eq!(out["results"].as_array().unwrap().len(), 1);
        assert!(out["error"].is_string());
    }

    #[test]
    fn server_status_round_trip() {
        let shared = db();
        let status = run_json(&shared, json!({"action": "server-status"}));
        assert_eq!(status["running"], false);
        let start = run_json(&shared, json!({"action": "server-start", "port": 0}));
        // In-memory databases cannot host a server.
        assert!(start.get("error").is_some());
    }
}
