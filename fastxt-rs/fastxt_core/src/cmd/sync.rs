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

use crate::{Note, OneString};
use rusqlite::Connection;
use std::collections::HashSet;
use tracing::warn;

/// Fetch a note by its UUID4. Returns a default empty `Note` if not found.
pub fn get_note_by_uuid4(conn: &Connection, uuid4: &str) -> Note {
    conn.query_row(
        "select uuid4, txt, tags, created_at, json(ai_tags), ai_summary, ai_category FROM note where uuid4 = ? ",
        [uuid4],
        |row| {
            Ok(Note {
                rowid: 0,
                uuid4: row.get(0)?,
                txt: row.get(1)?,
                tags: row.get(2)?,
                created_at: row.get(3)?,
                ai_tags: row.get(4)?,
                ai_summary: row.get(5)?,
                ai_category: row.get(6)?,
            })
        },
    )
    .unwrap_or_else(|e| {
        warn!(uuid4, error = %e, "failed to get note by uuid4");
        Note::default()
    })
}

/// Return all UUID4s in the database ordered by rowid (used as sync candidates).
pub fn next_uuid4_candidates(conn: &Connection) -> Vec<String> {
    let mut stmt = match conn.prepare("select uuid4 FROM note order by rowid") {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare next_uuid4_candidates");
            return Vec::new();
        }
    };
    match stmt.query_map([], |row| Ok(OneString { s: row.get(0)? })) {
        Ok(rows) => rows.flatten().map(|u| u.s).collect(),
        Err(e) => {
            warn!(error = %e, "failed to query uuid4 candidates");
            Vec::new()
        }
    }
}

/// Return the subset of `candidates` that are **not** present in this database.
/// Used by the client to ask the server which notes it is missing.
pub fn diff_uuid4_to_server(conn: &Connection, candidates: Vec<String>) -> Vec<String> {
    let mut stmt = match conn.prepare("select 1 FROM note where uuid4 = ? ") {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare diff_uuid4_to_server");
            return Vec::new();
        }
    };
    candidates
        .into_iter()
        .filter(|uuid4| !stmt.exists([&uuid4]).unwrap_or(false))
        .collect()
}

/// Return the UUID4s present locally but **not** in `candidates` (the server's list).
/// Used by the client to determine which notes to pull from the server.
pub fn diff_uuid4_from_server(conn: &Connection, candidates: &[String]) -> Vec<String> {
    let candidates: HashSet<_> = candidates.iter().collect();
    let mut stmt = match conn.prepare("select uuid4 FROM note") {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare diff_uuid4_from_server");
            return Vec::new();
        }
    };
    match stmt.query_map([], |row| Ok(OneString { s: row.get(0)? })) {
        Ok(rows) => rows
            .flatten()
            .filter(|u| !candidates.contains(&u.s))
            .map(|u| u.s)
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to query uuid4 from server");
            Vec::new()
        }
    }
}
