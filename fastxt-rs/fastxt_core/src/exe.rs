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

use crate::cmd::create;
use crate::cmd::delete;
use crate::cmd::insert;
use crate::cmd::search::{search, search_count};
use crate::cmd::select::select;
use crate::cmd::{
    select_notes_without_ai_tags, select_notes_without_embeddings, semantic_search,
    store_embedding, update_ai_tags, update_ai_summary,
};
use crate::upgrade;
use crate::AiEmbedResponse;
use crate::AiSummarizeResponse;
use crate::AiTagsResponse;
use crate::Cmd;
use crate::CmdAiEmbed;
use crate::CmdAiEmbedAll;
use crate::CmdAiSummarize;
use crate::CmdAiTag;
use crate::CmdAiTagAll;
use crate::CmdDelete;
use crate::CmdInsert;
use crate::CmdRpcClient;
use crate::CmdRpcServer;
use crate::CmdSearch;
use crate::CmdSelect;
use crate::CmdSemanticSearch;
use crate::Note;
use crate::SemanticSearchResponse;
use crate::SemanticSearchResult;
use chrono;
use chrono::prelude::Utc;
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use uuid::Uuid;

pub fn get_sqlite_connection() -> Connection {
    let p = sqlite3_db_location();
    let path = Path::new(&p);
    let conn = Connection::open(path).unwrap();
    conn
}

fn sqlite3_db_location() -> String {
    if cfg!(target_os = "android") {
        fs::create_dir_all("/sdcard/Fastxt").unwrap();
        return "/sdcard/Fastxt/fastxt.sqlite3".to_string();
    }
    let mut dir_name = "Fastxt";
    if cfg!(target_os = "ios") {
        dir_name = "Documents";
    }
    let dir = format!(
        "{}/{}",
        dirs::home_dir().unwrap().to_str().unwrap(),
        dir_name
    );
    eprintln!("db dir location: {}", dir);
    if !Path::new(&dir).exists() {
        fs::create_dir_all(&dir).unwrap();
    }
    format!("{}/fastxt.sqlite3", dir)
}

pub fn run(text: &str) -> String {
    if let Ok(cmd) = serde_json::from_str::<Cmd>(text) {
        process(cmd, text)
    } else {
        r#"{"error": "cmd json error"}"#.to_string()
    }
}

fn process(cmd: Cmd, text: &str) -> String {
    eprintln!("process cmd {:?}", cmd);
    let conn = get_sqlite_connection();
    create(&conn);

    // always run upgrade first
    if let Ok(version) = upgrade::upgrade(&conn) {
        eprintln!(r#"{{"upgrade-done": "{}"}}"#, version)
    } else {
        return r#"{"error":"upgrade error"}"#.to_string();
    }

    match cmd.action.as_ref() {
        "server-addr" => {
            // format!(r#"{{"addr": "{}"}}"#, crate::rpc::server::get_server_addr())
            crate::rpc::server::get_server_addr()
        }
        "server" => {
            eprintln!(r#"{{"server": "starting"}}"#);
            if let Ok(s) = serde_json::from_str::<CmdRpcServer>(text) {
                if let Ok(_) = crate::rpc::server::start(&s.addr) {
                    format!(r#"{{"server": "started"}}"#)
                } else {
                    r#"{"error":"server error"}"#.to_string()
                }
            } else {
                r#"{"error":"cmd server error"}"#.to_string()
            }
        }
        "search" => {
            if let Ok(s) = serde_json::from_str::<CmdSearch>(text) {
                do_search(&conn, &s.query, &s.limit, &s.offset)
            } else {
                r#"{"error":"cmd search json error"}"#.to_string()
            }
        }
        "select" => {
            if let Ok(s) = serde_json::from_str::<CmdSelect>(text) {
                do_select(&conn, &s.limit, &s.offset)
            } else {
                r#"{"error":"cmd select json error"}"#.to_string()
            }
        }
        "insert" => {
            if let Ok(i) = serde_json::from_str::<CmdInsert>(text) {
                let created_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
                let note = Note {
                    rowid: 0i64,
                    uuid4: Uuid::new_v4().to_string(),
                    txt: i.txt,
                    tags: i.tags,
                    created_at: created_at,
                };
                eprint!("{:?}", note);
                insert(&conn, note);
                do_select(&conn, &i.limit, &i.offset)
            } else {
                r#"{"error":"cmd insert json error"}"#.to_string()
            }
        }
        "delete" => {
            if let Ok(s) = serde_json::from_str::<CmdDelete>(text) {
                delete(&conn, s.rowid);
                do_search(&conn, &s.query, &s.limit, &s.offset)
            } else {
                r#"{"error":"cmd delete json error"}"#.to_string()
            }
        }
        "client-sync" => {
            eprintln!(r#"{{"client": "starting"}}"#);
            if let Ok(s) = serde_json::from_str::<CmdRpcClient>(text) {
                if let Ok(resp) = crate::rpc::client::sync(&s.addr) {
                    format!(r#"{{"client-sync": "{}"}}"#, resp)
                } else {
                    r#"{"error":"client-sync error"}"#.to_string()
                }
            } else {
                r#"{"error":"cmd client-sync error"}"#.to_string()
            }
        }
        "client-stop-server" => {
            eprintln!(r#"{{"client": "starting"}}"#);
            if let Ok(s) = serde_json::from_str::<CmdRpcClient>(text) {
                if let Ok(resp) = crate::rpc::client::stop_server(&s.addr) {
                    format!(r#"{{"client-stop-server": "{}"}}"#, resp)
                } else {
                    r#"{"error":"client-stop-server error"}"#.to_string()
                }
            } else {
                r#"{"error":"cmd client-stop-server error"}"#.to_string()
            }
        }
        "ai-tag" => {
            if let Ok(cmd) = serde_json::from_str::<CmdAiTag>(text) {
                do_ai_tag(&cmd)
            } else {
                r#"{"error":"cmd ai-tag json error"}"#.to_string()
            }
        }
        "ai-tag-all" => {
            if let Ok(cmd) = serde_json::from_str::<CmdAiTagAll>(text) {
                do_ai_tag_all(&conn, &cmd)
            } else {
                r#"{"error":"cmd ai-tag-all json error"}"#.to_string()
            }
        }
        "ai-summarize" => {
            if let Ok(cmd) = serde_json::from_str::<CmdAiSummarize>(text) {
                do_ai_summarize(&conn, &cmd)
            } else {
                r#"{"error":"cmd ai-summarize json error"}"#.to_string()
            }
        }
        "ai-embed" => {
            if let Ok(cmd) = serde_json::from_str::<CmdAiEmbed>(text) {
                do_ai_embed(&conn, &cmd)
            } else {
                r#"{"error":"cmd ai-embed json error"}"#.to_string()
            }
        }
        "ai-embed-all" => {
            if let Ok(cmd) = serde_json::from_str::<CmdAiEmbedAll>(text) {
                do_ai_embed_all(&conn, &cmd)
            } else {
                r#"{"error":"cmd ai-embed-all json error"}"#.to_string()
            }
        }
        "semantic-search" => {
            if let Ok(cmd) = serde_json::from_str::<CmdSemanticSearch>(text) {
                do_semantic_search(&conn, &cmd)
            } else {
                r#"{"error":"cmd semantic-search json error"}"#.to_string()
            }
        }
        _ => r#"{"error": "cmd no match"}"#.to_string(),
    }
}

fn do_search(conn: &Connection, query: &str, limit: &u32, offset: &u32) -> String {
    let c = search_count(&conn, query);
    let j = search(&conn, query, limit, offset);
    // let d = search_by_day(&conn, query);
    let d = "";
    // let t = search_by_tag(&conn, query);
    let t = "";
    let msg = format!(
        r#"{{"count": {}, "notes":{}}}"#,
        // r#"{{"count": {}, "notes":{}, "days": {}, "tags": {} }}"#,
        // c, j, d, t
        c,
        j
    );
    // eprintln!("msg {}", msg);
    msg
}

fn do_select(conn: &Connection, limit: &u32, offset: &u32) -> String {
    //    let c = select_count(&conn);
    let c = "";
    let j = select(&conn, limit, offset);
    //    let d = select_by_day(&conn);
    let d = "";
    //    let t = select_by_tag(&conn);
    let t = "";
    let msg = format!(
        r#"{{"notes":{}}}"#,
        //r#"{{"count": {}, "notes":{}, "days": {}, "tags": {} }}"#,
        // c, j, d, t
        j
    );
    // eprintln!("msg {}", msg);
    msg
}

/// Handle ai-tag command - suggest tags for given text.
fn do_ai_tag(cmd: &CmdAiTag) -> String {
    #[cfg(feature = "ai")]
    {
        use crate::ai::{get_default_backend, AiBackend, AiConfig};

        let config = AiConfig {
            endpoint: cmd.endpoint.clone(),
            model: cmd.model.clone(),
            ..Default::default()
        };

        let backend = get_default_backend();

        if !backend.is_available() {
            let response = AiTagsResponse {
                tags: vec![],
                available: false,
                error: Some("AI backend not available. Make sure Ollama is running.".to_string()),
            };
            return serde_json::to_string(&response).unwrap();
        }

        match backend.suggest_tags(&cmd.text, &config) {
            Ok(tags) => {
                let response = AiTagsResponse {
                    tags,
                    available: true,
                    error: None,
                };
                serde_json::to_string(&response).unwrap()
            }
            Err(e) => {
                let response = AiTagsResponse {
                    tags: vec![],
                    available: true,
                    error: Some(e.to_string()),
                };
                serde_json::to_string(&response).unwrap()
            }
        }
    }

    #[cfg(not(feature = "ai"))]
    {
        let response = AiTagsResponse {
            tags: vec![],
            available: false,
            error: Some("AI feature not enabled. Build with --features ai".to_string()),
        };
        serde_json::to_string(&response).unwrap()
    }
}

/// Handle ai-tag-all command - batch tag all notes without AI tags.
fn do_ai_tag_all(conn: &Connection, cmd: &CmdAiTagAll) -> String {
    #[cfg(feature = "ai")]
    {
        use crate::ai::{get_default_backend, AiBackend, AiConfig};

        let config = AiConfig {
            endpoint: cmd.endpoint.clone(),
            model: cmd.model.clone(),
            ..Default::default()
        };

        let backend = get_default_backend();

        if !backend.is_available() {
            return r#"{"error":"AI backend not available. Make sure Ollama is running."}"#
                .to_string();
        }

        let limit = cmd.limit.unwrap_or(50);
        let notes = select_notes_without_ai_tags(conn, limit);

        if notes.is_empty() {
            return r#"{"processed":0,"message":"No notes without AI tags"}"#.to_string();
        }

        let mut processed = 0;
        let mut errors = 0;

        for note in notes {
            match backend.suggest_tags(&note.txt, &config) {
                Ok(tags) => {
                    let tags_json = serde_json::to_string(&tags).unwrap();
                    update_ai_tags(conn, note.rowid, &tags_json);
                    processed += 1;
                }
                Err(e) => {
                    eprintln!("Failed to tag note {}: {}", note.rowid, e);
                    errors += 1;
                }
            }
        }

        format!(
            r#"{{"processed":{},"errors":{},"message":"Batch tagging complete"}}"#,
            processed, errors
        )
    }

    #[cfg(not(feature = "ai"))]
    {
        r#"{"error":"AI feature not enabled. Build with --features ai"}"#.to_string()
    }
}

/// Handle ai-summarize command - generate summary for a note.
fn do_ai_summarize(conn: &Connection, cmd: &CmdAiSummarize) -> String {
    #[cfg(feature = "ai")]
    {
        use crate::ai::{get_default_backend, AiBackend, AiConfig};

        let config = AiConfig {
            endpoint: cmd.endpoint.clone(),
            model: cmd.model.clone(),
            ..Default::default()
        };

        let backend = get_default_backend();

        if !backend.is_available() {
            let response = crate::AiSummarizeResponse {
                summary: None,
                available: false,
                error: Some("AI backend not available. Make sure Ollama is running.".to_string()),
            };
            return serde_json::to_string(&response).unwrap();
        }

        // Fetch the note text
        let txt: Option<String> = conn
            .query_row(
                "SELECT txt FROM note WHERE rowid = ?1",
                &[&cmd.rowid],
                |row| row.get(0),
            )
            .ok();

        match txt {
            Some(text) => match backend.summarize(&text, &config) {
                Ok(summary) => {
                    update_ai_summary(conn, cmd.rowid, &summary);
                    let response = crate::AiSummarizeResponse {
                        summary: Some(summary),
                        available: true,
                        error: None,
                    };
                    serde_json::to_string(&response).unwrap()
                }
                Err(e) => {
                    let response = crate::AiSummarizeResponse {
                        summary: None,
                        available: true,
                        error: Some(e.to_string()),
                    };
                    serde_json::to_string(&response).unwrap()
                }
            },
            None => {
                let response = crate::AiSummarizeResponse {
                    summary: None,
                    available: true,
                    error: Some(format!("Note with rowid {} not found", cmd.rowid)),
                };
                serde_json::to_string(&response).unwrap()
            }
        }
    }

    #[cfg(not(feature = "ai"))]
    {
        let response = crate::AiSummarizeResponse {
            summary: None,
            available: false,
            error: Some("AI feature not enabled. Build with --features ai".to_string()),
        };
        serde_json::to_string(&response).unwrap()
    }
}

/// Handle ai-embed command - generate embedding for a single note.
fn do_ai_embed(conn: &Connection, cmd: &CmdAiEmbed) -> String {
    #[cfg(feature = "ai")]
    {
        use crate::ai::{get_default_backend, AiBackend, AiConfig};

        let config = AiConfig {
            endpoint: cmd.endpoint.clone(),
            model: cmd.model.clone(),
            ..Default::default()
        };

        let backend = get_default_backend();

        if !backend.is_available() {
            let response = AiEmbedResponse {
                success: false,
                available: false,
                error: Some("AI backend not available. Make sure Ollama is running.".to_string()),
            };
            return serde_json::to_string(&response).unwrap();
        }

        // Fetch the note text
        let txt: Option<String> = conn
            .query_row(
                "SELECT txt FROM note WHERE rowid = ?1",
                &[&cmd.rowid],
                |row| row.get(0),
            )
            .ok();

        match txt {
            Some(text) => match backend.embed(&text, &config) {
                Ok(embedding) => {
                    let model_id = config.model.clone().unwrap_or_else(|| "unknown".to_string());
                    store_embedding(conn, cmd.rowid, &embedding, &model_id);
                    let response = AiEmbedResponse {
                        success: true,
                        available: true,
                        error: None,
                    };
                    serde_json::to_string(&response).unwrap()
                }
                Err(e) => {
                    let response = AiEmbedResponse {
                        success: false,
                        available: true,
                        error: Some(e.to_string()),
                    };
                    serde_json::to_string(&response).unwrap()
                }
            },
            None => {
                let response = AiEmbedResponse {
                    success: false,
                    available: true,
                    error: Some(format!("Note with rowid {} not found", cmd.rowid)),
                };
                serde_json::to_string(&response).unwrap()
            }
        }
    }

    #[cfg(not(feature = "ai"))]
    {
        let response = AiEmbedResponse {
            success: false,
            available: false,
            error: Some("AI feature not enabled. Build with --features ai".to_string()),
        };
        serde_json::to_string(&response).unwrap()
    }
}

/// Handle ai-embed-all command - batch embed all notes without embeddings.
fn do_ai_embed_all(conn: &Connection, cmd: &CmdAiEmbedAll) -> String {
    #[cfg(feature = "ai")]
    {
        use crate::ai::{get_default_backend, AiBackend, AiConfig};

        let config = AiConfig {
            endpoint: cmd.endpoint.clone(),
            model: cmd.model.clone(),
            ..Default::default()
        };

        let backend = get_default_backend();

        if !backend.is_available() {
            return r#"{"error":"AI backend not available. Make sure Ollama is running."}"#
                .to_string();
        }

        let limit = cmd.limit.unwrap_or(50);
        let notes = select_notes_without_embeddings(conn, limit);

        if notes.is_empty() {
            return r#"{"processed":0,"message":"No notes without embeddings"}"#.to_string();
        }

        let model_id = config.model.clone().unwrap_or_else(|| "unknown".to_string());
        let mut processed = 0;
        let mut errors = 0;

        for note in notes {
            match backend.embed(&note.txt, &config) {
                Ok(embedding) => {
                    store_embedding(conn, note.rowid, &embedding, &model_id);
                    processed += 1;
                }
                Err(e) => {
                    eprintln!("Failed to embed note {}: {}", note.rowid, e);
                    errors += 1;
                }
            }
        }

        format!(
            r#"{{"processed":{},"errors":{},"message":"Batch embedding complete"}}"#,
            processed, errors
        )
    }

    #[cfg(not(feature = "ai"))]
    {
        r#"{"error":"AI feature not enabled. Build with --features ai"}"#.to_string()
    }
}

/// Handle semantic-search command - find notes by meaning.
fn do_semantic_search(conn: &Connection, cmd: &CmdSemanticSearch) -> String {
    #[cfg(feature = "ai")]
    {
        use crate::ai::{get_default_backend, AiBackend, AiConfig};

        let config = AiConfig {
            endpoint: cmd.endpoint.clone(),
            model: cmd.model.clone(),
            ..Default::default()
        };

        let backend = get_default_backend();

        if !backend.is_available() {
            let response = SemanticSearchResponse {
                results: vec![],
                available: false,
                error: Some("AI backend not available. Make sure Ollama is running.".to_string()),
            };
            return serde_json::to_string(&response).unwrap();
        }

        // Generate embedding for query
        match backend.embed(&cmd.query, &config) {
            Ok(query_embedding) => {
                let model_id = config.model.clone().unwrap_or_else(|| "unknown".to_string());
                let limit = cmd.limit.unwrap_or(10);
                let threshold = cmd.threshold.unwrap_or(0.5);

                let results = semantic_search(conn, &query_embedding, &model_id, limit, threshold);

                let response = SemanticSearchResponse {
                    results: results
                        .into_iter()
                        .map(|r| SemanticSearchResult {
                            note: r.note,
                            similarity: r.similarity,
                        })
                        .collect(),
                    available: true,
                    error: None,
                };
                serde_json::to_string(&response).unwrap()
            }
            Err(e) => {
                let response = SemanticSearchResponse {
                    results: vec![],
                    available: true,
                    error: Some(format!("Failed to generate query embedding: {}", e)),
                };
                serde_json::to_string(&response).unwrap()
            }
        }
    }

    #[cfg(not(feature = "ai"))]
    {
        let response = SemanticSearchResponse {
            results: vec![],
            available: false,
            error: Some("AI feature not enabled. Build with --features ai".to_string()),
        };
        serde_json::to_string(&response).unwrap()
    }
}
