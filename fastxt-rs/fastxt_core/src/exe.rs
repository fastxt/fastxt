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
use crate::cmd::insert;
use crate::cmd::search::{search, search_count};
use crate::cmd::select::select;
use crate::Cmd;
use crate::CmdInsert;
use crate::CmdSearch;
use crate::CmdSelect;
use crate::Note;
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
        fs::create_dir_all("/sdcard/LocalNative").unwrap();
        return "/sdcard/LocalNative/fastxt.sqlite3".to_string();
    }
    let mut dir_name = "LocalNative";
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

    match cmd.action.as_ref() {
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
                let note = Note {
                    rowid: 0i64,
                    uuid4: Uuid::new_v4().to_string(),
                    txt: i.txt,
                    tags: i.tags,
                    created_at: "".to_string(), // db will use default value
                };
                eprint!("{:?}", note);
                insert(&conn, note);
                do_select(&conn, &i.limit, &i.offset)
            } else {
                r#"{"error":"cmd insert json error"}"#.to_string()
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
