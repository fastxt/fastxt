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
use rusqlite::Connection;

pub fn create(conn: &Connection) {
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS note (
         rowid          INTEGER PRIMARY KEY AUTOINCREMENT,
         uuid4          TEXT NOT NULL UNIQUE,
         txt            TEXT NOT NULL,
         tags           TEXT NOT NULL,
         created_at     TEXT NOT NULL,
         ai_tags        TEXT,
         ai_summary     TEXT,
         ai_category    TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_created_at
         ON note (created_at);
         CREATE TABLE IF NOT EXISTS meta (
         meta_key        TEXT PRIMARY KEY,
         meta_value      TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS note_embedding (
         note_rowid     INTEGER PRIMARY KEY REFERENCES note(rowid),
         embedding      BLOB NOT NULL,
         model_id       TEXT NOT NULL,
         created_at     TEXT NOT NULL
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
    conn.execute(
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

/// Migrate database to add AI columns if they don't exist.
/// Called during upgrade process.
pub fn migrate_ai_columns(conn: &Connection) {
    // Add AI columns to note table if they don't exist
    let columns = ["ai_tags", "ai_summary", "ai_category"];
    for col in &columns {
        let check_sql = format!(
            "SELECT COUNT(*) FROM pragma_table_info('note') WHERE name='{}'",
            col
        );
        let count: i32 = conn.query_row(&check_sql, [], |row| row.get(0)).unwrap_or(0);
        if count == 0 {
            let alter_sql = format!("ALTER TABLE note ADD COLUMN {} TEXT", col);
            if let Err(e) = conn.execute(&alter_sql, []) {
                eprintln!("Warning: Failed to add column {}: {}", col, e);
            } else {
                eprintln!("Added column {} to note table", col);
            }
        }
    }

    // Create note_embedding table if it doesn't exist
    if let Err(e) = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS note_embedding (
         note_rowid     INTEGER PRIMARY KEY REFERENCES note(rowid),
         embedding      BLOB NOT NULL,
         model_id       TEXT NOT NULL,
         created_at     TEXT NOT NULL
         );",
    ) {
        eprintln!("Warning: Failed to create note_embedding table: {}", e);
    }
}

/// Update AI tags for a note.
pub fn update_ai_tags(conn: &Connection, rowid: i64, ai_tags: &str) {
    if let Err(e) = conn.execute(
        "UPDATE note SET ai_tags = ?1 WHERE rowid = ?2",
        rusqlite::params![ai_tags, rowid],
    ) {
        eprintln!("Failed to update ai_tags: {}", e);
    }
}

/// Update AI summary for a note.
pub fn update_ai_summary(conn: &Connection, rowid: i64, ai_summary: &str) {
    if let Err(e) = conn.execute(
        "UPDATE note SET ai_summary = ?1 WHERE rowid = ?2",
        rusqlite::params![ai_summary, rowid],
    ) {
        eprintln!("Failed to update ai_summary: {}", e);
    }
}

/// Update AI category for a note.
pub fn update_ai_category(conn: &Connection, rowid: i64, ai_category: &str) {
    if let Err(e) = conn.execute(
        "UPDATE note SET ai_category = ?1 WHERE rowid = ?2",
        rusqlite::params![ai_category, rowid],
    ) {
        eprintln!("Failed to update ai_category: {}", e);
    }
}

/// Get notes without AI tags (for batch processing).
pub fn select_notes_without_ai_tags(conn: &Connection, limit: u32) -> Vec<crate::Note> {
    let mut stmt = conn
        .prepare(
            "SELECT rowid, uuid4, txt, tags, created_at
             FROM note
             WHERE ai_tags IS NULL OR ai_tags = ''
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .unwrap();

    let notes = stmt
        .query_map(&[&limit], |row| {
            Ok(crate::Note {
                rowid: row.get(0)?,
                uuid4: row.get(1)?,
                txt: row.get(2)?,
                tags: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .unwrap()
        .filter_map(|n| n.ok())
        .collect();

    notes
}

/// Store embedding for a note.
pub fn store_embedding(conn: &Connection, note_rowid: i64, embedding: &[f32], model_id: &str) {
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let embedding_bytes: Vec<u8> = embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect();

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO note_embedding (note_rowid, embedding, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![note_rowid, embedding_bytes, model_id, created_at],
    ) {
        eprintln!("Failed to store embedding: {}", e);
    }
}
