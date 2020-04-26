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

use crate::Note;
use linked_hash_set::LinkedHashSet;
use regex::Regex;
use std::iter::FromIterator;
pub mod search;
pub mod select;
pub mod sync;
use rusqlite::{Connection, NO_PARAMS};
use uuid::Uuid;

pub fn create(conn: &Connection) {
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS note (
         rowid          INTEGER PRIMARY KEY AUTOINCREMENT,
         uuid4          TEXT NOT NULL UNIQUE,
         txt            TEXT NOT NULL,
         tags           TEXT NOT NULL,
         created_at     TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_created_at
         ON note (created_at);
         CREATE TABLE IF NOT EXISTS meta (
         meta_key        TEXT PRIMARY KEY,
         meta_value      TEXT NOT NULL
         );
         COMMIT;",
    )
    .unwrap();
}

pub fn delete(conn: &Connection, rowid: i64) {
    eprintln!("delete rowid {}", rowid);
    conn.execute("delete from note where rowid = ?1", &[&rowid])
        .unwrap();
}

pub fn insert(conn: &Connection, note: Note) {
    conn.execute_named(
        "
        INSERT INTO note (uuid4, txt, tags, created_at)
        VALUES (:uuid4, :txt, :tags, :created_at);
        ",
        &[
            (":uuid4", &note.uuid4),
            (":txt", &note.txt),
            (":tags", &make_tags(&note.tags)),
            (":created_at", &note.created_at),
        ],
    )
    .unwrap();
}

// format and dedup tags
pub fn make_tags(input: &str) -> String {
    let re1 = Regex::new(r",+").unwrap();
    let re2 = Regex::new(r"\s+").unwrap();
    let s1 = re1.replace_all(input, " ");
    let s2 = re2.replace_all(s1.trim(), ",");
    let v1 = s2.split(",");
    let h1: LinkedHashSet<&str> = LinkedHashSet::from_iter(v1);
    let mut s = "".to_string();
    for e in h1 {
        s.push_str(e);
        s.push_str(",")
    }
    s.pop();
    s.to_string()
}
