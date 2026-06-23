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
use rusqlite::Connection;
use rusqlite::types::ToSql;
use tracing::warn;

/// Return the total number of notes in the database.
pub fn select_count(conn: &Connection) -> u32 {
    conn.query_row("SELECT count(1) FROM note", [], |row| row.get(0))
        .unwrap_or(0)
}

/// Return a JSON array of notes ordered by `created_at` descending.
pub fn select(conn: &Connection, limit: &u32, offset: &u32) -> String {
    let notes = select_imp(conn, limit, offset);
    serde_json::to_string(&notes).unwrap_or_else(|_| "[]".to_string())
}

/// Return a page of notes as a `Vec<Note>` (internal helper used by tests and search fallback).
pub fn select_imp(conn: &Connection, limit: &u32, offset: &u32) -> Vec<Note> {
    let mut stmt = match conn.prepare(
        "SELECT rowid, uuid4, txt, tags, created_at, json(ai_tags), ai_summary, ai_category
        FROM note
        order by created_at desc limit :limit offset :offset",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare select");
            return Vec::new();
        }
    };

    match stmt.query_map(
        &[
            (":limit", limit as &dyn ToSql),
            (":offset", offset as &dyn ToSql),
        ],
        |row| {
            Ok(Note {
                rowid: row.get(0)?,
                uuid4: row.get(1)?,
                txt: row.get(2)?,
                tags: row.get(3)?,
                created_at: row.get(4)?,
                ai_tags: row.get(5)?,
                ai_summary: row.get(6)?,
                ai_category: row.get(7)?,
            })
        },
    ) {
        Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
        Err(e) => {
            warn!(error = %e, "failed to query notes");
            Vec::new()
        }
    }
}
