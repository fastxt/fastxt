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
    // Handle AI tags merge: if both local and incoming have AI tags, union them
    let ai_tags = note.ai_tags.as_ref().map(|incoming| {
        // Check if there's an existing note with AI tags
        let existing_ai_tags: Option<String> = conn
            .query_row(
                "SELECT ai_tags FROM note WHERE uuid4 = ?1 AND ai_tags IS NOT NULL AND ai_tags != ''",
                rusqlite::params![&note.uuid4],
                |row| row.get(0),
            )
            .ok()
            .flatten();

        match existing_ai_tags {
            Some(existing) => merge_ai_tags(&existing, incoming),
            None => incoming.clone(),
        }
    });

    conn.execute(
        "
        INSERT OR REPLACE INTO note (uuid4, txt, tags, created_at, ai_tags, ai_summary, ai_category)
        VALUES (:uuid4, :txt, :tags, :created_at, :ai_tags, :ai_summary, :ai_category);
        ",
        rusqlite::named_params! {
            ":uuid4": &note.uuid4,
            ":txt": &note.txt,
            ":tags": &make_tags(&note.tags),
            ":created_at": &note.created_at,
            ":ai_tags": &ai_tags,
            ":ai_summary": &note.ai_summary,
            ":ai_category": &note.ai_category,
        },
    )
    .unwrap();
}

/// Merge AI tags by unioning them (for sync).
fn merge_ai_tags(existing: &str, incoming: &str) -> String {
    use std::collections::HashSet;

    let existing_tags: HashSet<String> = existing
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let incoming_tags: HashSet<String> = incoming
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();

    let merged: Vec<String> = existing_tags
        .union(&incoming_tags)
        .cloned()
        .collect();

    merged.join(",")
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
                ai_tags: None,
                ai_summary: None,
                ai_category: None,
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

/// Get embedding for a note by rowid.
pub fn get_embedding(conn: &Connection, note_rowid: i64) -> Option<(Vec<f32>, String)> {
    conn.query_row(
        "SELECT embedding, model_id FROM note_embedding WHERE note_rowid = ?1",
        rusqlite::params![note_rowid],
        |row| {
            let bytes: Vec<u8> = row.get(0)?;
            let model_id: String = row.get(1)?;
            // Convert bytes back to f32 vector
            let embedding: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            Ok((embedding, model_id))
        },
    )
    .ok()
}

/// Get all embeddings with their note rowids.
pub fn get_all_embeddings(conn: &Connection, model_id: Option<&str>) -> Vec<(i64, Vec<f32>)> {
    let sql = match model_id {
        Some(_) => "SELECT note_rowid, embedding FROM note_embedding WHERE model_id = ?1",
        None => "SELECT note_rowid, embedding FROM note_embedding",
    };

    let mut stmt = conn.prepare(sql).unwrap();
    let rows: Vec<(i64, Vec<u8>)> = match model_id {
        Some(mid) => stmt
            .query_map(&[mid], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect(),
        None => stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect(),
    };

    rows.into_iter()
        .map(|(rowid, bytes)| {
            let embedding: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect();
            (rowid, embedding)
        })
        .collect()
}

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    dot_product / (mag_a * mag_b)
}

/// Result of semantic search.
#[derive(Debug, Clone)]
pub struct SemanticSearchResult {
    pub note: Note,
    pub similarity: f32,
}

/// Search notes by semantic similarity to a query embedding.
pub fn semantic_search(
    conn: &Connection,
    query_embedding: &[f32],
    model_id: &str,
    limit: u32,
    threshold: f32,
) -> Vec<SemanticSearchResult> {
    // Get all embeddings for this model
    let embeddings = get_all_embeddings(conn, Some(model_id));

    if embeddings.is_empty() {
        return vec![];
    }

    // Compute similarities and sort
    let mut scored: Vec<(i64, f32)> = embeddings
        .iter()
        .map(|(rowid, embedding)| (*rowid, cosine_similarity(query_embedding, embedding)))
        .filter(|(_, score)| *score >= threshold)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit as usize);

    // Fetch notes for top results
    let rowids: Vec<i64> = scored.iter().map(|(r, _)| *r).collect();
    if rowids.is_empty() {
        return vec![];
    }

    let placeholders: String = rowids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at FROM note WHERE rowid IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql).unwrap();
    let params: Vec<&dyn rusqlite::ToSql> = rowids.iter().map(|r| r as &dyn rusqlite::ToSql).collect();

    let notes: std::collections::HashMap<i64, Note> = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Note {
                    rowid: row.get(0)?,
                    uuid4: row.get(1)?,
                    txt: row.get(2)?,
                    tags: row.get(3)?,
                    created_at: row.get(4)?,
                    ai_tags: None,
                    ai_summary: None,
                    ai_category: None,
                },
            ))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    // Combine with scores in order
    scored
        .into_iter()
        .filter_map(|(rowid, score)| {
            notes.get(&rowid).map(|note| SemanticSearchResult {
                note: note.clone(),
                similarity: score,
            })
        })
        .collect()
}

/// Count notes without embeddings (for batch embedding generation).
pub fn count_notes_without_embeddings(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM note n
         WHERE NOT EXISTS (SELECT 1 FROM note_embedding e WHERE e.note_rowid = n.rowid)",
        [],
        |row| row.get(0),
    )
    .unwrap_or(0)
}

/// Get notes without embeddings (for batch embedding generation).
pub fn select_notes_without_embeddings(conn: &Connection, limit: u32) -> Vec<crate::Note> {
    let mut stmt = conn
        .prepare(
            "SELECT n.rowid, n.uuid4, n.txt, n.tags, n.created_at
             FROM note n
             WHERE NOT EXISTS (SELECT 1 FROM note_embedding e WHERE e.note_rowid = n.rowid)
             ORDER BY n.created_at DESC
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
                ai_tags: None,
                ai_summary: None,
                ai_category: None,
            })
        })
        .unwrap()
        .filter_map(|n| n.ok())
        .collect();

    notes
}
