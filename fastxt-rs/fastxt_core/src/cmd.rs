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
use tracing::{debug, info, warn};
pub mod search;
pub mod select;
pub mod sync;
use rusqlite::Connection;

/// Default embedding dimension (all-MiniLM-L6-v2 produces 384-dim vectors).
pub const DEFAULT_EMBEDDING_DIM: usize = 384;

/// Register the sqlite-vec extension as an auto-extension.
/// Must be called before opening any database connections.
/// Safe to call multiple times — uses `std::sync::Once` internally.
pub fn register_sqlite_vec() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        unsafe {
            // Transmute follows the canonical pattern from the sqlite-vec crate documentation.
            #[allow(clippy::missing_transmute_annotations)]
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
        info!("sqlite-vec extension registered");
    });
}

/// Create the database schema (tables and indices) if they do not already exist.
///
/// # Panics
/// Panics if the database schema cannot be created (e.g., disk full or corrupt database).
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
    .expect("failed to create database schema");

    // Create the sqlite-vec virtual table for vector similarity search.
    // This must be done outside the transaction above because virtual table
    // creation cannot be rolled back.
    create_vec_table(conn, DEFAULT_EMBEDDING_DIM);

    // Create the FTS5 virtual table for full-text search.
    create_fts5_table(conn);
}

/// Create the vec_notes virtual table with the given embedding dimension.
/// Safe to call repeatedly — uses `IF NOT EXISTS` semantics via error handling.
pub fn create_vec_table(conn: &Connection, dim: usize) {
    let sql = format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_notes USING vec0(note_rowid INTEGER PRIMARY KEY, embedding float[{}])",
        dim
    );
    if let Err(e) = conn.execute_batch(&sql) {
        // If the table already exists (possibly with a different dimension), this is fine.
        debug!("vec_notes table creation note: {}", e);
    }
}

/// Create the FTS5 virtual table and sync triggers for full-text search.
/// The FTS5 table mirrors `txt` and `tags` from the `note` table.
/// Safe to call repeatedly — uses `IF NOT EXISTS`.
pub fn create_fts5_table(conn: &Connection) {
    if let Err(e) = conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS note_fts USING fts5(txt, tags, content=note, content_rowid=rowid);",
    ) {
        debug!("note_fts table creation note: {}", e);
    }

    // Create triggers to keep FTS5 in sync with the note table.
    if let Err(e) = conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS note_ai AFTER INSERT ON note BEGIN
            INSERT INTO note_fts(rowid, txt, tags) VALUES (new.rowid, new.txt, new.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS note_ad AFTER DELETE ON note BEGIN
            INSERT INTO note_fts(note_fts, rowid, txt, tags) VALUES('delete', old.rowid, old.txt, old.tags);
        END;
        CREATE TRIGGER IF NOT EXISTS note_au AFTER UPDATE ON note BEGIN
            INSERT INTO note_fts(note_fts, rowid, txt, tags) VALUES('delete', old.rowid, old.txt, old.tags);
            INSERT INTO note_fts(rowid, txt, tags) VALUES (new.rowid, new.txt, new.tags);
        END;",
    ) {
        debug!("note_fts triggers creation note: {}", e);
    }
}

/// Delete a note and its associated embedding by rowid.
///
/// # Panics
/// Panics if the DELETE statement fails (e.g., database is read-only or locked).
pub fn delete(conn: &Connection, rowid: i64) {
    debug!(rowid, "deleting note");
    // Delete from vec_notes (sqlite-vec) first
    let _ = conn.execute("DELETE FROM vec_notes WHERE note_rowid = ?1", [&rowid]);
    // Delete associated embedding to avoid orphaned data
    let _ = conn.execute("DELETE FROM note_embedding WHERE note_rowid = ?1", [&rowid]);
    conn.execute("DELETE FROM note WHERE rowid = ?1", [&rowid])
        .expect("failed to delete note");
}

/// Insert or replace a note, merging AI tags when both sides have them.
///
/// Uses `INSERT OR REPLACE` keyed on `uuid4`, so syncing the same note
/// from another device is idempotent. AI tags from both sides are unioned.
///
/// # Panics
/// Panics if the INSERT statement fails (e.g., database is read-only or locked).
pub fn insert(conn: &Connection, note: &Note) {
    // Handle AI tags merge: if both local and incoming have AI tags, union them
    let ai_tags = note.ai_tags.as_ref().map(|incoming| {
        // Check if there's an existing note with AI tags
        let existing_ai_tags: Option<String> = conn
            .query_row(
                "SELECT json(ai_tags) FROM note WHERE uuid4 = ?1 AND ai_tags IS NOT NULL AND ai_tags != ''",
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
    .expect("failed to insert/replace note");
}

/// Merge AI tags by unioning them (for sync).
/// AI tags are stored as JSON arrays (e.g., `["rust","programming"]`).
fn merge_ai_tags(existing: &str, incoming: &str) -> String {
    use std::collections::HashSet;

    let parse_tags = |s: &str| -> HashSet<String> {
        // Try JSON array first
        if let Ok(tags) = serde_json::from_str::<Vec<String>>(s) {
            return tags.into_iter().map(|t| t.to_lowercase()).collect();
        }
        // Fallback to comma-separated for backwards compatibility
        s.split(',')
            .map(|t| t.trim().trim_matches('"').to_lowercase())
            .filter(|t| !t.is_empty())
            .collect()
    };

    let existing_tags = parse_tags(existing);
    let incoming_tags = parse_tags(incoming);

    let merged: Vec<String> = existing_tags.union(&incoming_tags).cloned().collect();

    serde_json::to_string(&merged).unwrap_or_else(|_| incoming.to_string())
}

/// Normalise a raw tags string: collapse separators, deduplicate, and return
/// a comma-separated list with no trailing comma.
///
/// Both spaces and commas are treated as tag delimiters; duplicate tags are
/// removed while preserving insertion order.
pub fn make_tags(input: &str) -> String {
    use std::sync::LazyLock;
    static RE_COMMAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",+").unwrap());
    static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let s1 = RE_COMMAS.replace_all(input, " ");
    let s2 = RE_SPACES.replace_all(s1.trim(), ",");
    let v1 = s2.split(',');
    let h1: LinkedHashSet<&str> = v1.collect();
    let mut s = String::new();
    for e in h1 {
        s.push_str(e);
        s.push(',');
    }
    s.pop();
    s
}

/// Migrate database to add AI columns if they don't exist.
/// Called during upgrade process.
pub fn migrate_ai_columns(conn: &Connection) {
    // Add AI columns to note table if they don't exist
    let columns = ["ai_tags", "ai_summary", "ai_category"];
    for col in &columns {
        let check_sql =
            format!("SELECT COUNT(*) FROM pragma_table_info('note') WHERE name='{col}'");
        let count: i32 = conn
            .query_row(&check_sql, [], |row| row.get(0))
            .unwrap_or(0);
        if count == 0 {
            let alter_sql = format!("ALTER TABLE note ADD COLUMN {col} TEXT");
            if let Err(e) = conn.execute(&alter_sql, []) {
                warn!(column = col, error = %e, "failed to add column");
            } else {
                info!(column = col, "added column to note table");
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
        warn!(error = %e, "failed to create note_embedding table");
    }
}

/// Migrate database to add FTS5 virtual table and populate from existing notes.
/// Called during upgrade process.
pub fn migrate_fts5(conn: &Connection) {
    // Create the FTS5 virtual table and triggers.
    create_fts5_table(conn);

    // Populate FTS5 from existing notes.
    if let Err(e) = conn
        .execute_batch("INSERT INTO note_fts(rowid, txt, tags) SELECT rowid, txt, tags FROM note;")
    {
        // May fail if already populated (e.g., duplicate rowids) — that is fine.
        debug!("FTS5 population note: {}", e);
    } else {
        info!("populated note_fts from existing notes");
    }
}

/// Update AI tags for a note, storing as JSONB for compact binary representation.
///
/// Uses SQLite's `jsonb()` function (available since SQLite 3.45) to store
/// ai_tags in binary JSONB format. This is more compact and faster to query
/// than plain JSON text. Use `json(ai_tags)` when reading to convert back to text.
pub fn update_ai_tags(conn: &Connection, rowid: i64, ai_tags: &str) {
    if let Err(e) = conn.execute(
        "UPDATE note SET ai_tags = jsonb(?1) WHERE rowid = ?2",
        rusqlite::params![ai_tags, rowid],
    ) {
        warn!(rowid, error = %e, "failed to update ai_tags");
    }
}

/// Extract AI tags for a note using `json_each()` for efficient JSONB access.
///
/// Returns the tags as a `Vec<String>`, leveraging SQLite's `json_each()` to
/// iterate over the JSONB array without deserializing the entire blob.
pub fn json_extract_tags(conn: &Connection, rowid: i64) -> Vec<String> {
    let mut stmt = match conn.prepare(
        "SELECT je.value FROM note, json_each(json(note.ai_tags)) AS je WHERE note.rowid = ?1",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(rowid, error = %e, "failed to prepare json_extract_tags");
            return Vec::new();
        }
    };

    match stmt.query_map(rusqlite::params![rowid], |row| row.get::<_, String>(0)) {
        Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
        Err(e) => {
            warn!(rowid, error = %e, "failed to extract tags via json_each");
            Vec::new()
        }
    }
}

/// Update AI summary for a note.
pub fn update_ai_summary(conn: &Connection, rowid: i64, ai_summary: &str) {
    if let Err(e) = conn.execute(
        "UPDATE note SET ai_summary = ?1 WHERE rowid = ?2",
        rusqlite::params![ai_summary, rowid],
    ) {
        warn!(rowid, error = %e, "failed to update ai_summary");
    }
}

/// Update AI category for a note.
pub fn update_ai_category(conn: &Connection, rowid: i64, ai_category: &str) {
    if let Err(e) = conn.execute(
        "UPDATE note SET ai_category = ?1 WHERE rowid = ?2",
        rusqlite::params![ai_category, rowid],
    ) {
        warn!(rowid, error = %e, "failed to update ai_category");
    }
}

/// Get notes without AI tags (for batch processing).
pub fn select_notes_without_ai_tags(conn: &Connection, limit: u32) -> Vec<crate::Note> {
    let mut stmt = match conn.prepare(
        "SELECT rowid, uuid4, txt, tags, created_at
         FROM note
         WHERE ai_tags IS NULL OR ai_tags = ''
         ORDER BY created_at DESC
         LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare select_notes_without_ai_tags");
            return Vec::new();
        }
    };

    let result = match stmt.query_map([&limit], |row| {
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
    }) {
        Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
        Err(e) => {
            warn!(error = %e, "failed to query notes without AI tags");
            Vec::new()
        }
    };
    result
}

/// Store embedding for a note in both note_embedding (for model_id tracking)
/// and vec_notes (for fast vector similarity search via sqlite-vec).
pub fn store_embedding(conn: &Connection, note_rowid: i64, embedding: &[f32], model_id: &str) {
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let embedding_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO note_embedding (note_rowid, embedding, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![note_rowid, embedding_bytes, model_id, created_at],
    ) {
        warn!(error = %e, "failed to store embedding in note_embedding");
    }

    // Also store in vec_notes for sqlite-vec vector search.
    // Remove existing entry first (vec0 does not support OR REPLACE).
    let _ = conn.execute(
        "DELETE FROM vec_notes WHERE note_rowid = ?1",
        rusqlite::params![note_rowid],
    );
    let vec_bytes = zerocopy::IntoBytes::as_bytes(embedding);
    if let Err(e) = conn.execute(
        "INSERT INTO vec_notes(note_rowid, embedding) VALUES (?1, ?2)",
        rusqlite::params![note_rowid, vec_bytes],
    ) {
        warn!(error = %e, "failed to store embedding in vec_notes");
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

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare get_all_embeddings");
            return Vec::new();
        }
    };
    let rows: Vec<(i64, Vec<u8>)> = match model_id {
        Some(mid) => match stmt.query_map([mid], |row| Ok((row.get(0)?, row.get(1)?))) {
            Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
            Err(e) => {
                warn!(error = %e, "failed to query embeddings");
                Vec::new()
            }
        },
        None => match stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) {
            Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
            Err(e) => {
                warn!(error = %e, "failed to query embeddings");
                Vec::new()
            }
        },
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
#[must_use]
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

/// Search notes by semantic similarity to a query embedding using sqlite-vec.
///
/// Uses the `vec_notes` virtual table for fast nearest-neighbor search instead
/// of loading all embeddings into memory. Falls back to the pure-Rust approach
/// if the vec_notes table is empty (e.g., before migration).
///
/// The `model_id` parameter is currently unused by the sqlite-vec query (the
/// vec_notes table does not track model_id), but kept for API compatibility.
/// The `threshold` is applied as a post-filter on the distance returned by
/// sqlite-vec (which returns L2 distance; we convert to cosine similarity).
pub fn semantic_search(
    conn: &Connection,
    query_embedding: &[f32],
    _model_id: &str,
    limit: u32,
    threshold: f32,
) -> Vec<SemanticSearchResult> {
    let query_bytes = zerocopy::IntoBytes::as_bytes(query_embedding);

    // Use sqlite-vec's MATCH query for fast vector similarity search.
    // sqlite-vec returns L2 (Euclidean) distance by default.
    // We fetch more than `limit` to allow post-filtering by threshold.
    let fetch_limit = limit * 4;
    let mut stmt = match conn.prepare(
        "SELECT v.note_rowid, v.distance, n.uuid4, n.txt, n.tags, n.created_at
         FROM vec_notes v
         INNER JOIN note n ON n.rowid = v.note_rowid
         WHERE v.embedding MATCH ?1
         ORDER BY v.distance
         LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare sqlite-vec semantic search, falling back");
            return semantic_search_fallback(conn, query_embedding, _model_id, limit, threshold);
        }
    };

    let results: Vec<SemanticSearchResult> =
        match stmt.query_map(rusqlite::params![query_bytes, fetch_limit], |row| {
            let distance: f64 = row.get(1)?;
            Ok((
                distance,
                Note {
                    rowid: row.get(0)?,
                    uuid4: row.get(2)?,
                    txt: row.get(3)?,
                    tags: row.get(4)?,
                    created_at: row.get(5)?,
                    ai_tags: None,
                    ai_summary: None,
                    ai_category: None,
                },
            ))
        }) {
            Ok(rows) => rows
                .filter_map(std::result::Result::ok)
                .filter_map(|(distance, note)| {
                    // Convert L2 distance to a similarity-like score.
                    // similarity = 1 / (1 + distance) gives a value in (0, 1].
                    let similarity = 1.0 / (1.0 + distance as f32);
                    if similarity >= threshold {
                        Some(SemanticSearchResult { note, similarity })
                    } else {
                        None
                    }
                })
                .take(limit as usize)
                .collect(),
            Err(e) => {
                warn!(error = %e, "sqlite-vec semantic search query failed, falling back");
                return semantic_search_fallback(
                    conn,
                    query_embedding,
                    _model_id,
                    limit,
                    threshold,
                );
            }
        };

    results
}

/// Fallback semantic search using pure-Rust cosine similarity.
/// Used when the vec_notes table is not available or empty.
fn semantic_search_fallback(
    conn: &Connection,
    query_embedding: &[f32],
    model_id: &str,
    limit: u32,
    threshold: f32,
) -> Vec<SemanticSearchResult> {
    let embeddings = get_all_embeddings(conn, Some(model_id));

    if embeddings.is_empty() {
        return vec![];
    }

    let mut scored: Vec<(i64, f32)> = embeddings
        .iter()
        .map(|(rowid, embedding)| (*rowid, cosine_similarity(query_embedding, embedding)))
        .filter(|(_, score)| *score >= threshold)
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit as usize);

    let rowids: Vec<i64> = scored.iter().map(|(r, _)| *r).collect();
    if rowids.is_empty() {
        return vec![];
    }

    let placeholders: String = rowids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at FROM note WHERE rowid IN ({placeholders})"
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare fallback semantic_search note fetch");
            return vec![];
        }
    };
    let params: Vec<&dyn rusqlite::ToSql> =
        rowids.iter().map(|r| r as &dyn rusqlite::ToSql).collect();

    let notes: std::collections::HashMap<i64, Note> =
        match stmt.query_map(params.as_slice(), |row| {
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
        }) {
            Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
            Err(e) => {
                warn!(error = %e, "failed to query notes for fallback semantic search");
                std::collections::HashMap::new()
            }
        };

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

/// Find notes related to a given note by embedding similarity using sqlite-vec.
///
/// Returns a list of `(note_rowid, distance)` pairs ordered by ascending distance
/// (most similar first). The source note is excluded from results.
/// Returns an empty `Vec` if the note has no embedding.
pub fn find_related_notes(conn: &Connection, note_rowid: i64, limit: i64) -> Vec<(i64, f64)> {
    // 1. Get the embedding for the given note from note_embedding table.
    let embedding_bytes: Vec<u8> = match conn.query_row(
        "SELECT embedding FROM note_embedding WHERE note_rowid = ?1",
        rusqlite::params![note_rowid],
        |row| row.get(0),
    ) {
        Ok(bytes) => bytes,
        Err(_) => return Vec::new(),
    };

    // 2. Query vec_notes for nearest neighbors, excluding the source note.
    //    Fetch extra rows because we need to filter out the source note.
    let fetch_limit = limit + 1;
    let mut stmt = match conn.prepare(
        "SELECT note_rowid, distance FROM vec_notes WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare find_related_notes query");
            return Vec::new();
        }
    };

    match stmt.query_map(rusqlite::params![embedding_bytes, fetch_limit], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
    }) {
        Ok(rows) => rows
            .filter_map(std::result::Result::ok)
            .filter(|(rowid, _)| *rowid != note_rowid)
            .take(limit as usize)
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to query find_related_notes");
            Vec::new()
        }
    }
}

/// Find notes related to a given note, returning full [`Note`] objects.
///
/// This is a convenience wrapper around [`find_related_notes`] that fetches
/// the complete note data for each related result.
pub fn find_related_notes_full(conn: &Connection, note_rowid: i64, limit: i64) -> Vec<Note> {
    let related = find_related_notes(conn, note_rowid, limit);
    if related.is_empty() {
        return Vec::new();
    }

    let rowids: Vec<i64> = related.iter().map(|(r, _)| *r).collect();
    let placeholders: String = rowids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at, ai_tags, ai_summary, ai_category
         FROM note WHERE rowid IN ({placeholders})"
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare find_related_notes_full note fetch");
            return Vec::new();
        }
    };
    let params: Vec<&dyn rusqlite::ToSql> =
        rowids.iter().map(|r| r as &dyn rusqlite::ToSql).collect();

    let notes_map: std::collections::HashMap<i64, Note> =
        match stmt.query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                Note {
                    rowid: row.get(0)?,
                    uuid4: row.get(1)?,
                    txt: row.get(2)?,
                    tags: row.get(3)?,
                    created_at: row.get(4)?,
                    ai_tags: row.get(5)?,
                    ai_summary: row.get(6)?,
                    ai_category: row.get(7)?,
                },
            ))
        }) {
            Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
            Err(e) => {
                warn!(error = %e, "failed to query notes for find_related_notes_full");
                return Vec::new();
            }
        };

    // Preserve the distance-based ordering from find_related_notes.
    related
        .into_iter()
        .filter_map(|(rowid, _)| notes_map.get(&rowid).cloned())
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
    let mut stmt = match conn.prepare(
        "SELECT n.rowid, n.uuid4, n.txt, n.tags, n.created_at
         FROM note n
         WHERE NOT EXISTS (SELECT 1 FROM note_embedding e WHERE e.note_rowid = n.rowid)
         ORDER BY n.created_at DESC
         LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare select_notes_without_embeddings");
            return Vec::new();
        }
    };

    let result = match stmt.query_map([&limit], |row| {
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
    }) {
        Ok(rows) => rows.filter_map(std::result::Result::ok).collect(),
        Err(e) => {
            warn!(error = %e, "failed to query notes without embeddings");
            Vec::new()
        }
    };
    result
}

/// Rename a category for all notes that have it.
/// Returns the number of notes updated.
pub fn rename_category(conn: &Connection, old_name: &str, new_name: &str) -> usize {
    match conn.execute(
        "UPDATE note SET ai_category = ?1 WHERE ai_category = ?2",
        rusqlite::params![new_name, old_name],
    ) {
        Ok(rows_affected) => rows_affected,
        Err(e) => {
            warn!(error = %e, "failed to rename category");
            0
        }
    }
}

/// Dismiss (clear) the category for all notes with the given category.
/// Returns the number of notes updated.
pub fn dismiss_category(conn: &Connection, category: &str) -> usize {
    match conn.execute(
        "UPDATE note SET ai_category = NULL WHERE ai_category = ?1",
        rusqlite::params![category],
    ) {
        Ok(rows_affected) => rows_affected,
        Err(e) => {
            warn!(error = %e, "failed to dismiss category");
            0
        }
    }
}

/// Get all categories with their note counts.
pub fn get_categories(conn: &Connection) -> std::collections::HashMap<String, usize> {
    let mut stmt = match conn.prepare(
        "SELECT ai_category, COUNT(*) FROM note WHERE ai_category IS NOT NULL AND ai_category != '' GROUP BY ai_category"
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to get categories");
            return std::collections::HashMap::new();
        }
    };

    let Ok(rows) = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
    }) else {
        return std::collections::HashMap::new();
    };

    rows.filter_map(std::result::Result::ok)
        .map(|(cat, count)| (cat, count as usize))
        .collect()
}

/// Get the embedding model ID from the database.
/// Returns the most recently used model ID, or None if no embeddings exist.
pub fn get_embedding_model_id(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT model_id FROM note_embedding ORDER BY created_at DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .ok()
}

/// Get all note UUID4s that have embeddings with a specific model ID.
pub fn get_embedding_uuid4s_by_model(conn: &Connection, model_id: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT n.uuid4 FROM note n
         INNER JOIN note_embedding e ON n.rowid = e.note_rowid
         WHERE e.model_id = ?1",
    ) else {
        return Vec::new();
    };

    let Ok(rows) = stmt.query_map([model_id], |row| row.get::<_, String>(0)) else {
        return Vec::new();
    };

    rows.filter_map(std::result::Result::ok).collect()
}

/// Get embedding data by note UUID4.
/// Returns (`embedding_bytes`, `model_id`) if found.
pub fn get_embedding_by_uuid4(conn: &Connection, uuid4: &str) -> Option<(Vec<u8>, String)> {
    conn.query_row(
        "SELECT e.embedding, e.model_id FROM note_embedding e
         INNER JOIN note n ON e.note_rowid = n.rowid
         WHERE n.uuid4 = ?1",
        [uuid4],
        |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
    )
    .ok()
}

/// Store embedding data by note UUID4.
pub fn store_embedding_by_uuid4(
    conn: &Connection,
    uuid4: &str,
    embedding_bytes: &[u8],
    model_id: &str,
) {
    // First get the note rowid from uuid4
    let note_rowid: i64 =
        match conn.query_row("SELECT rowid FROM note WHERE uuid4 = ?1", [uuid4], |row| {
            row.get(0)
        }) {
            Ok(id) => id,
            Err(e) => {
                warn!(uuid4, error = %e, "failed to find note by uuid4");
                return;
            }
        };

    // Then store the embedding in note_embedding
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO note_embedding (note_rowid, embedding, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![note_rowid, embedding_bytes, model_id, created_at],
    ) {
        warn!(error = %e, "failed to store embedding in note_embedding");
    }

    // Also store in vec_notes for sqlite-vec vector search.
    let _ = conn.execute(
        "DELETE FROM vec_notes WHERE note_rowid = ?1",
        rusqlite::params![note_rowid],
    );
    if let Err(e) = conn.execute(
        "INSERT INTO vec_notes(note_rowid, embedding) VALUES (?1, ?2)",
        rusqlite::params![note_rowid, embedding_bytes],
    ) {
        warn!(error = %e, "failed to store embedding in vec_notes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Note;

    fn setup_test_db() -> Connection {
        register_sqlite_vec();
        let conn = Connection::open_in_memory().unwrap();
        create(&conn);
        conn
    }

    fn make_test_note(txt: &str, tags: &str) -> Note {
        Note {
            rowid: 0,
            uuid4: uuid::Uuid::new_v4().to_string(),
            txt: txt.to_string(),
            tags: tags.to_string(),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            ai_tags: None,
            ai_summary: None,
            ai_category: None,
        }
    }

    #[test]
    fn test_create_schema() {
        let conn = setup_test_db();
        let count: i64 = conn
            .query_row("SELECT count(1) FROM note", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = conn
            .query_row("SELECT count(1) FROM meta", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_insert_and_select() {
        let conn = setup_test_db();
        let note = make_test_note("hello world", "test,rust");
        let uuid = note.uuid4.clone();
        insert(&conn, &note);

        let notes = select::select_imp(&conn, &10, &0);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].uuid4, uuid);
        assert_eq!(notes[0].txt, "hello world");
    }

    #[test]
    fn test_insert_multiple_and_count() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("note 1", "a"));
        insert(&conn, &make_test_note("note 2", "b"));
        insert(&conn, &make_test_note("note 3", "c"));
        assert_eq!(select::select_count(&conn), 3);
    }

    #[test]
    fn test_delete() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("to delete", "x"));
        let notes = select::select_imp(&conn, &10, &0);
        assert_eq!(notes.len(), 1);
        delete(&conn, notes[0].rowid);
        assert_eq!(select::select_count(&conn), 0);
    }

    #[test]
    fn test_search() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("rust programming", "code"));
        insert(&conn, &make_test_note("python scripting", "code"));
        insert(&conn, &make_test_note("cooking recipes", "food"));

        let result = search::search(&conn, "rust", &10, &0);
        let notes: Vec<Note> = serde_json::from_str(&result).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].txt, "rust programming");
    }

    #[test]
    fn test_search_count() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("rust lang", "code"));
        insert(&conn, &make_test_note("rust book", "code"));
        insert(&conn, &make_test_note("python book", "code"));
        assert_eq!(search::search_count(&conn, "rust"), 2);
        assert_eq!(search::search_count(&conn, "python"), 1);
        assert_eq!(search::search_count(&conn, "book"), 2);
    }

    #[test]
    fn test_search_empty_query_returns_all() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("note 1", "a"));
        insert(&conn, &make_test_note("note 2", "b"));
        assert_eq!(search::search_count(&conn, ""), 2);
    }

    #[test]
    fn test_make_tags_dedup() {
        let result = make_tags("a,b,a");
        let tags: Vec<&str> = result.split(',').collect();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"a"));
        assert!(tags.contains(&"b"));

        let result = make_tags("rust  code  rust");
        let tags: Vec<&str> = result.split(',').collect();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"rust"));
        assert!(tags.contains(&"code"));
    }

    #[test]
    fn test_make_tags_whitespace() {
        assert_eq!(make_tags("a  b  c"), "a,b,c");
        assert_eq!(make_tags("a,,b,,c"), "a,b,c");
    }

    #[test]
    fn test_insert_upsert_by_uuid4() {
        let conn = setup_test_db();
        let note = make_test_note("original", "v1");
        let uuid = note.uuid4.clone();
        let created_at = note.created_at.clone();
        insert(&conn, &note);

        let updated = Note {
            rowid: 0,
            uuid4: uuid,
            txt: "updated".to_string(),
            tags: "v2".to_string(),
            created_at,
            ai_tags: None,
            ai_summary: None,
            ai_category: None,
        };
        insert(&conn, &updated);
        assert_eq!(select::select_count(&conn), 1);
        let notes = select::select_imp(&conn, &10, &0);
        assert_eq!(notes[0].txt, "updated");
    }

    #[test]
    fn test_select_pagination() {
        let conn = setup_test_db();
        for i in 0..5 {
            insert(&conn, &make_test_note(&format!("note {}", i), "tag"));
        }
        assert_eq!(select::select_imp(&conn, &2, &0).len(), 2);
        assert_eq!(select::select_imp(&conn, &2, &2).len(), 2);
        assert_eq!(select::select_imp(&conn, &2, &4).len(), 1);
    }

    #[test]
    fn test_sync_get_note_by_uuid4() {
        let conn = setup_test_db();
        let note = make_test_note("sync test", "sync");
        let uuid = note.uuid4.clone();
        insert(&conn, &note);
        let found = sync::get_note_by_uuid4(&conn, &uuid);
        assert_eq!(found.txt, "sync test");
    }

    #[test]
    fn test_sync_diff_uuid4() {
        let conn = setup_test_db();
        let n1 = make_test_note("one", "a");
        let n2 = make_test_note("two", "b");
        let uuid1 = n1.uuid4.clone();
        let uuid2 = n2.uuid4.clone();
        insert(&conn, &n1);
        insert(&conn, &n2);

        let uuid3 = uuid::Uuid::new_v4().to_string();
        let missing = sync::diff_uuid4_to_server(&conn, vec![uuid1.clone(), uuid3.clone()]);
        assert_eq!(missing, vec![uuid3]);

        let from = sync::diff_uuid4_from_server(&conn, &[uuid1]);
        assert_eq!(from, vec![uuid2]);
    }

    #[test]
    fn test_store_and_get_embedding() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("embed me", "test"));
        let notes = select::select_imp(&conn, &1, &0);
        let rowid = notes[0].rowid;

        store_embedding(&conn, rowid, &[0.1f32, 0.2, 0.3, 0.4], "test-model");

        let (retrieved, model) = get_embedding(&conn, rowid).unwrap();
        assert_eq!(model, "test-model");
        assert_eq!(retrieved.len(), 4);
        assert!((retrieved[0] - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn test_cosine_similarity() {
        assert!((cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < f32::EPSILON);
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < f32::EPSILON);
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn test_categories() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("rust code", "code"));
        insert(&conn, &make_test_note("python code", "code"));
        insert(&conn, &make_test_note("recipe", "food"));

        let notes = select::select_imp(&conn, &10, &0);
        update_ai_category(&conn, notes[0].rowid, "programming");
        update_ai_category(&conn, notes[1].rowid, "programming");
        update_ai_category(&conn, notes[2].rowid, "cooking");

        let cats = get_categories(&conn);
        assert_eq!(*cats.get("programming").unwrap(), 2);
        assert_eq!(*cats.get("cooking").unwrap(), 1);

        assert_eq!(rename_category(&conn, "cooking", "culinary"), 1);
        assert!(get_categories(&conn).get("cooking").is_none());
        assert_eq!(*get_categories(&conn).get("culinary").unwrap(), 1);

        assert_eq!(dismiss_category(&conn, "culinary"), 1);
        assert!(get_categories(&conn).get("culinary").is_none());
    }

    #[test]
    fn test_merge_ai_tags() {
        let result = merge_ai_tags(r#"["rust","code"]"#, r#"["python","code"]"#);
        let tags: Vec<String> = serde_json::from_str(&result).unwrap();
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"python".to_string()));
        assert!(tags.contains(&"code".to_string()));
    }

    // ---- Sync edge-case tests ----

    #[test]
    fn test_sync_next_uuid4_candidates_order() {
        let conn = setup_test_db();
        let n1 = make_test_note("first", "a");
        let n2 = make_test_note("second", "b");
        let n3 = make_test_note("third", "c");
        let uuid1 = n1.uuid4.clone();
        let uuid2 = n2.uuid4.clone();
        let uuid3 = n3.uuid4.clone();
        insert(&conn, &n1);
        insert(&conn, &n2);
        insert(&conn, &n3);

        let candidates = sync::next_uuid4_candidates(&conn);
        assert_eq!(candidates.len(), 3);
        // Returned in rowid order (insertion order for this test)
        assert_eq!(candidates[0], uuid1);
        assert_eq!(candidates[1], uuid2);
        assert_eq!(candidates[2], uuid3);
    }

    #[test]
    fn test_sync_diff_to_server_all_known() {
        // If the server already has all UUIDs, nothing is missing.
        let conn = setup_test_db();
        let n = make_test_note("exists", "x");
        let uuid = n.uuid4.clone();
        insert(&conn, &n);
        let missing = sync::diff_uuid4_to_server(&conn, vec![uuid]);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_sync_diff_to_server_empty_candidates() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("local only", "tag"));
        // Server sends empty candidate list — local note is not "missing" from server
        let missing = sync::diff_uuid4_to_server(&conn, vec![]);
        assert!(missing.is_empty());
    }

    #[test]
    fn test_sync_diff_from_server_all_present() {
        // If every server UUID is also local, nothing to pull.
        let conn = setup_test_db();
        let n = make_test_note("shared", "s");
        let uuid = n.uuid4.clone();
        insert(&conn, &n);
        let to_pull = sync::diff_uuid4_from_server(&conn, &[uuid]);
        assert!(to_pull.is_empty());
    }

    #[test]
    fn test_sync_diff_from_server_empty_server_list() {
        // Server has nothing; client should return all local UUIDs as "to pull from server"
        // Note: diff_uuid4_from_server returns local UUIDs NOT in the server list.
        let conn = setup_test_db();
        let n = make_test_note("local", "tag");
        let uuid = n.uuid4.clone();
        insert(&conn, &n);
        let to_pull = sync::diff_uuid4_from_server(&conn, &[]);
        assert_eq!(to_pull, vec![uuid]);
    }

    #[test]
    fn test_sync_upsert_idempotent() {
        // Inserting the same UUID twice should not duplicate the row.
        let conn = setup_test_db();
        let note = make_test_note("original", "v1");
        insert(&conn, &note);
        insert(&conn, &note); // same UUID
        assert_eq!(select::select_count(&conn), 1);
    }

    #[test]
    fn test_make_tags_empty_string() {
        // An empty input should produce an empty string, not panic.
        let result = make_tags("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_make_tags_whitespace_only() {
        let result = make_tags("   ");
        // Trimming + dedup; exact output may be empty or whitespace-collapsed
        assert!(!result.contains("  "));
    }

    #[test]
    fn test_make_tags_trailing_comma() {
        let result = make_tags("a,b,");
        let tags: Vec<&str> = result.split(',').filter(|s| !s.is_empty()).collect();
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0f32, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_mismatched_len() {
        let sim = cosine_similarity(&[1.0, 2.0], &[1.0]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_update_ai_fields() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("ai test", "tag"));
        let notes = select::select_imp(&conn, &1, &0);
        let rowid = notes[0].rowid;

        update_ai_tags(&conn, rowid, r#"["ai","test"]"#);
        update_ai_summary(&conn, rowid, "a short summary");
        update_ai_category(&conn, rowid, "science");

        let note = sync::get_note_by_uuid4(&conn, &notes[0].uuid4);
        assert_eq!(note.ai_tags.as_deref(), Some(r#"["ai","test"]"#));
        assert_eq!(note.ai_summary.as_deref(), Some("a short summary"));
        assert_eq!(note.ai_category.as_deref(), Some("science"));
    }

    #[test]
    fn test_merge_ai_tags_union_dedup() {
        // Duplicate tags across both sides should be deduplicated.
        let result = merge_ai_tags(r#"["a","b"]"#, r#"["b","c"]"#);
        let tags: Vec<String> = serde_json::from_str(&result).unwrap();
        let unique: std::collections::HashSet<_> = tags.iter().collect();
        assert_eq!(tags.len(), unique.len(), "tags should be deduplicated");
    }

    #[test]
    fn test_merge_ai_tags_comma_separated_fallback() {
        // Fallback: if existing tags are comma-separated (legacy format), they should be unioned.
        let result = merge_ai_tags("rust,code", r#"["python","code"]"#);
        let tags: Vec<String> = serde_json::from_str(&result).unwrap();
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"python".to_string()));
    }

    // ---- find_related_notes tests ----

    /// Build a 384-dimensional test embedding with the first few values set.
    /// The rest are zero-padded to match DEFAULT_EMBEDDING_DIM.
    fn make_test_embedding(leading: &[f32]) -> Vec<f32> {
        let mut v = vec![0.0f32; DEFAULT_EMBEDDING_DIM];
        for (i, &val) in leading.iter().enumerate() {
            v[i] = val;
        }
        v
    }

    #[test]
    fn test_find_related_notes_ordered_by_similarity() {
        let conn = setup_test_db();

        // Insert 4 notes with embeddings of increasing distance from note 1.
        let n1 = make_test_note("rust basics", "rust");
        let n2 = make_test_note("rust advanced", "rust");
        let n3 = make_test_note("python basics", "python");
        let n4 = make_test_note("cooking pasta", "food");
        insert(&conn, &n1);
        insert(&conn, &n2);
        insert(&conn, &n3);
        insert(&conn, &n4);

        let notes = select::select_imp(&conn, &10, &0);
        let r1 = notes.iter().find(|n| n.txt == "rust basics").unwrap().rowid;
        let r2 = notes
            .iter()
            .find(|n| n.txt == "rust advanced")
            .unwrap()
            .rowid;
        let r3 = notes
            .iter()
            .find(|n| n.txt == "python basics")
            .unwrap()
            .rowid;
        let r4 = notes
            .iter()
            .find(|n| n.txt == "cooking pasta")
            .unwrap()
            .rowid;

        // Embeddings: n1 and n2 are very similar, n3 is moderately close, n4 is far.
        store_embedding(
            &conn,
            r1,
            &make_test_embedding(&[1.0, 0.0, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r2,
            &make_test_embedding(&[0.9, 0.1, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r3,
            &make_test_embedding(&[0.5, 0.5, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r4,
            &make_test_embedding(&[0.0, 0.0, 1.0, 0.0]),
            "test",
        );

        let related = find_related_notes(&conn, r1, 3);
        assert_eq!(related.len(), 3, "should return 3 related notes");

        // Verify ordering: n2 should be closest to n1, then n3, then n4.
        assert_eq!(related[0].0, r2, "n2 should be most similar to n1");
        assert_eq!(related[1].0, r3, "n3 should be second most similar to n1");
        assert_eq!(related[2].0, r4, "n4 should be least similar to n1");

        // Distances should be ascending.
        assert!(
            related[0].1 <= related[1].1,
            "distances should be ascending"
        );
        assert!(
            related[1].1 <= related[2].1,
            "distances should be ascending"
        );
    }

    #[test]
    fn test_find_related_notes_excludes_self() {
        let conn = setup_test_db();

        let n1 = make_test_note("only note", "test");
        insert(&conn, &n1);
        let notes = select::select_imp(&conn, &1, &0);
        let r1 = notes[0].rowid;

        store_embedding(
            &conn,
            r1,
            &make_test_embedding(&[1.0, 0.0, 0.0, 0.0]),
            "test",
        );

        let related = find_related_notes(&conn, r1, 5);
        assert!(
            related.is_empty(),
            "should return empty when only the source note exists"
        );
    }

    #[test]
    fn test_find_related_notes_no_embedding() {
        let conn = setup_test_db();

        let n1 = make_test_note("no embedding", "test");
        insert(&conn, &n1);
        let notes = select::select_imp(&conn, &1, &0);
        let r1 = notes[0].rowid;

        // Do not store any embedding for this note.
        let related = find_related_notes(&conn, r1, 5);
        assert!(
            related.is_empty(),
            "should return empty when note has no embedding"
        );
    }

    #[test]
    fn test_find_related_notes_respects_limit() {
        let conn = setup_test_db();

        let n1 = make_test_note("source", "a");
        let n2 = make_test_note("near", "b");
        let n3 = make_test_note("far", "c");
        let n4 = make_test_note("farther", "d");
        insert(&conn, &n1);
        insert(&conn, &n2);
        insert(&conn, &n3);
        insert(&conn, &n4);

        let notes = select::select_imp(&conn, &10, &0);
        let r1 = notes.iter().find(|n| n.txt == "source").unwrap().rowid;
        let r2 = notes.iter().find(|n| n.txt == "near").unwrap().rowid;
        let r3 = notes.iter().find(|n| n.txt == "far").unwrap().rowid;
        let r4 = notes.iter().find(|n| n.txt == "farther").unwrap().rowid;

        store_embedding(
            &conn,
            r1,
            &make_test_embedding(&[1.0, 0.0, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r2,
            &make_test_embedding(&[0.9, 0.1, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r3,
            &make_test_embedding(&[0.5, 0.5, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r4,
            &make_test_embedding(&[0.0, 0.0, 0.0, 1.0]),
            "test",
        );

        // Request only 1 result.
        let related = find_related_notes(&conn, r1, 1);
        assert_eq!(related.len(), 1, "should respect limit of 1");
        assert_eq!(related[0].0, r2, "should return the closest note");
    }

    #[test]
    fn test_find_related_notes_full_returns_note_objects() {
        let conn = setup_test_db();

        let n1 = make_test_note("source note", "src");
        let n2 = make_test_note("related note", "rel");
        insert(&conn, &n1);
        insert(&conn, &n2);

        let notes = select::select_imp(&conn, &10, &0);
        let r1 = notes.iter().find(|n| n.txt == "source note").unwrap().rowid;
        let r2 = notes
            .iter()
            .find(|n| n.txt == "related note")
            .unwrap()
            .rowid;

        store_embedding(
            &conn,
            r1,
            &make_test_embedding(&[1.0, 0.0, 0.0, 0.0]),
            "test",
        );
        store_embedding(
            &conn,
            r2,
            &make_test_embedding(&[0.9, 0.1, 0.0, 0.0]),
            "test",
        );

        let full = find_related_notes_full(&conn, r1, 5);
        assert_eq!(full.len(), 1, "should return 1 related note");
        assert_eq!(full[0].txt, "related note");
        assert_eq!(full[0].rowid, r2);
    }

    // ---- FTS5 hybrid search tests ----

    #[test]
    fn test_fts5_table_created() {
        let conn = setup_test_db();
        let count: i64 = conn
            .query_row(
                "SELECT count(1) FROM sqlite_master WHERE type='table' AND name='note_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "note_fts table should exist");
    }

    #[test]
    fn test_fts5_search_basic() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("rust programming language", "code"));
        insert(&conn, &make_test_note("python scripting language", "code"));
        insert(&conn, &make_test_note("cooking delicious recipes", "food"));

        let results = search::hybrid_search(&conn, "rust", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.txt, "rust programming language");
    }

    #[test]
    fn test_fts5_search_multiple_words() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("rust systems programming", "code"));
        insert(&conn, &make_test_note("rust game development", "games"));
        insert(&conn, &make_test_note("python programming", "code"));

        let results = search::hybrid_search(&conn, "rust programming", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.txt, "rust systems programming");
    }

    #[test]
    fn test_hybrid_search_text_only() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("machine learning basics", "ai"));
        insert(&conn, &make_test_note("deep learning neural nets", "ai"));
        insert(&conn, &make_test_note("cooking pasta recipes", "food"));

        let results = search::hybrid_search(&conn, "learning", None, 10);
        assert_eq!(results.len(), 2);
        let texts: Vec<&str> = results.iter().map(|r| r.note.txt.as_str()).collect();
        assert!(texts.contains(&"machine learning basics"));
        assert!(texts.contains(&"deep learning neural nets"));
    }

    #[test]
    fn test_hybrid_search_with_vector() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("alpha note", "first"));
        insert(&conn, &make_test_note("beta note", "second"));
        insert(&conn, &make_test_note("gamma note", "third"));

        let notes = select::select_imp(&conn, &10, &0);

        let emb1 = make_test_embedding(&[1.0, 0.0, 0.0, 0.0]);
        let emb2 = make_test_embedding(&[0.0, 1.0, 0.0, 0.0]);
        let emb3 = make_test_embedding(&[0.9, 0.1, 0.0, 0.0]);

        for note in &notes {
            let emb = if note.txt == "alpha note" {
                &emb1
            } else if note.txt == "beta note" {
                &emb2
            } else {
                &emb3
            };
            store_embedding(&conn, note.rowid, emb, "test-model");
        }

        let query_emb = make_test_embedding(&[1.0, 0.0, 0.0, 0.0]);

        let results = search::hybrid_search(&conn, "alpha", Some(&query_emb), 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].note.txt, "alpha note");
    }

    #[test]
    fn test_rrf_scoring_combines_sources() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("unique alpha text", "tag1"));
        insert(&conn, &make_test_note("unique beta text", "tag2"));

        let notes = select::select_imp(&conn, &10, &0);

        let emb_alpha = make_test_embedding(&[0.1, 0.0, 0.0, 0.0]);
        let emb_beta = make_test_embedding(&[1.0, 0.0, 0.0, 0.0]);

        for note in &notes {
            let emb = if note.txt.contains("alpha") {
                &emb_alpha
            } else {
                &emb_beta
            };
            store_embedding(&conn, note.rowid, emb, "test-model");
        }

        let query_emb = make_test_embedding(&[1.0, 0.0, 0.0, 0.0]);

        let results = search::hybrid_search(&conn, "alpha", Some(&query_emb), 10);
        assert!(!results.is_empty());

        let texts: Vec<&str> = results.iter().map(|r| r.note.txt.as_str()).collect();
        assert!(texts.contains(&"unique alpha text"));
        assert!(texts.contains(&"unique beta text"));

        for result in &results {
            assert!(result.score > 0.0);
        }
    }

    #[test]
    fn test_hybrid_search_no_embeddings_graceful() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("test note here", "example"));

        let results = search::hybrid_search(&conn, "test", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.txt, "test note here");
    }

    #[test]
    fn test_hybrid_search_empty_query() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("some note", "tag"));

        let results = search::hybrid_search(&conn, "", None, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fts5_sync_on_delete() {
        let conn = setup_test_db();
        insert(&conn, &make_test_note("deletable note", "temp"));
        let notes = select::select_imp(&conn, &1, &0);
        let rowid = notes[0].rowid;

        let results = search::hybrid_search(&conn, "deletable", None, 10);
        assert_eq!(results.len(), 1);

        delete(&conn, rowid);
        let results = search::hybrid_search(&conn, "deletable", None, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_fts5_sync_on_update() {
        let conn = setup_test_db();
        let note = make_test_note("original text", "v1");
        let uuid = note.uuid4.clone();
        let created_at = note.created_at.clone();
        insert(&conn, &note);

        let results = search::hybrid_search(&conn, "original", None, 10);
        assert_eq!(results.len(), 1);

        let updated = Note {
            rowid: 0,
            uuid4: uuid,
            txt: "modified text".to_string(),
            tags: "v2".to_string(),
            created_at,
            ai_tags: None,
            ai_summary: None,
            ai_category: None,
        };
        insert(&conn, &updated);

        let results = search::hybrid_search(&conn, "original", None, 10);
        assert!(results.is_empty());

        let results = search::hybrid_search(&conn, "modified", None, 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.txt, "modified text");
    }

    #[test]
    fn test_migrate_fts5_populates_existing() {
        register_sqlite_vec();
        let conn = Connection::open_in_memory().unwrap();

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS note (
             rowid          INTEGER PRIMARY KEY AUTOINCREMENT,
             uuid4          TEXT NOT NULL UNIQUE,
             txt            TEXT NOT NULL,
             tags           TEXT NOT NULL,
             created_at     TEXT NOT NULL,
             ai_tags        TEXT,
             ai_summary     TEXT,
             ai_category    TEXT
             );
             CREATE TABLE IF NOT EXISTS meta (
             meta_key        TEXT PRIMARY KEY,
             meta_value      TEXT NOT NULL
             );",
        )
        .unwrap();

        conn.execute(
            "INSERT INTO note (uuid4, txt, tags, created_at) VALUES ('u1', 'existing note one', 'old', '2024-01-01 00:00:00')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO note (uuid4, txt, tags, created_at) VALUES ('u2', 'existing note two', 'old', '2024-01-01 00:00:01')",
            [],
        )
        .unwrap();

        migrate_fts5(&conn);

        let results = search::hybrid_search(&conn, "existing", None, 10);
        assert_eq!(results.len(), 2);
    }
}
