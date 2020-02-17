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
use crate::cmd::select::select;
use crate::Cmd;
use crate::CmdSelect;
use rusqlite::Connection;
use std::fs;
use std::path::Path;

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
    fs::create_dir_all(&dir).unwrap();
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
        "select" => {
            if let Ok(s) = serde_json::from_str::<CmdSelect>(text) {
                do_select(&conn, &s.limit, &s.offset)
            } else {
                r#"{"error":"cmd select json error"}"#.to_string()
            }
        }
        _ => r#"{"error": "cmd no match"}"#.to_string(),
    }
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
        r#"{{"count": {}, "notes":{}, "days": {}, "tags": {} }}"#,
        c, j, d, t
    );
    // eprintln!("msg {}", msg);
    msg
}
