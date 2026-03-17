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
    .expect("failed to create database schema");
}

pub fn delete(conn: &Connection, rowid: i64) {
    eprintln!("delete rowid {}", rowid);
    // Delete associated embedding first to avoid orphaned data
    let _ = conn.execute("DELETE FROM note_embedding WHERE note_rowid = ?1", [&rowid]);
    conn.execute("DELETE FROM note WHERE rowid = ?1", [&rowid])
        .expect("failed to delete note");
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

// format and dedup tags
pub fn make_tags(input: &str) -> String {
    use std::sync::LazyLock;
    static RE_COMMAS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",+").unwrap());
    static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let s1 = RE_COMMAS.replace_all(input, " ");
    let s2 = RE_SPACES.replace_all(s1.trim(), ",");
    let v1 = s2.split(",");
    let h1: LinkedHashSet<&str> = LinkedHashSet::from_iter(v1);
    let mut s = "".to_string();
    for e in h1 {
        s.push_str(e);
        s.push(',')
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
        let count: i32 = conn
            .query_row(&check_sql, [], |row| row.get(0))
            .unwrap_or(0);
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
    let mut stmt = match conn.prepare(
        "SELECT rowid, uuid4, txt, tags, created_at
         FROM note
         WHERE ai_tags IS NULL OR ai_tags = ''
         ORDER BY created_at DESC
         LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to prepare select_notes_without_ai_tags: {}", e);
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
        Ok(rows) => rows.filter_map(|n| n.ok()).collect(),
        Err(e) => {
            eprintln!("Failed to query notes without AI tags: {}", e);
            Vec::new()
        }
    };
    result
}

/// Store embedding for a note.
pub fn store_embedding(conn: &Connection, note_rowid: i64, embedding: &[f32], model_id: &str) {
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let embedding_bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

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

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to prepare get_all_embeddings: {}", e);
            return Vec::new();
        }
    };
    let rows: Vec<(i64, Vec<u8>)> = match model_id {
        Some(mid) => match stmt.query_map([mid], |row| Ok((row.get(0)?, row.get(1)?))) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("Failed to query embeddings: {}", e);
                Vec::new()
            }
        },
        None => match stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) {
            Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                eprintln!("Failed to query embeddings: {}", e);
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

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to prepare semantic_search note fetch: {}", e);
            return vec![];
        }
    };
    let params: Vec<&dyn rusqlite::ToSql> =
        rowids.iter().map(|r| r as &dyn rusqlite::ToSql).collect();

    let notes: std::collections::HashMap<i64, Note> = match stmt.query_map(
        params.as_slice(),
        |row| {
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
        },
    ) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            eprintln!("Failed to query notes for semantic search: {}", e);
            std::collections::HashMap::new()
        }
    };

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
    let mut stmt = match conn.prepare(
        "SELECT n.rowid, n.uuid4, n.txt, n.tags, n.created_at
         FROM note n
         WHERE NOT EXISTS (SELECT 1 FROM note_embedding e WHERE e.note_rowid = n.rowid)
         ORDER BY n.created_at DESC
         LIMIT ?1",
    ) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to prepare select_notes_without_embeddings: {}", e);
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
        Ok(rows) => rows.filter_map(|n| n.ok()).collect(),
        Err(e) => {
            eprintln!("Failed to query notes without embeddings: {}", e);
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
            eprintln!("Failed to rename category: {}", e);
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
            eprintln!("Failed to dismiss category: {}", e);
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
            eprintln!("Failed to get categories: {}", e);
            return std::collections::HashMap::new();
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    }) {
        Ok(r) => r,
        Err(_) => return std::collections::HashMap::new(),
    };

    rows.filter_map(|r| r.ok())
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
    let mut stmt = match conn.prepare(
        "SELECT n.uuid4 FROM note n
         INNER JOIN note_embedding e ON n.rowid = e.note_rowid
         WHERE e.model_id = ?1",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map([model_id], |row| row.get::<_, String>(0)) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|r| r.ok()).collect()
}

/// Get embedding data by note UUID4.
/// Returns (embedding_bytes, model_id) if found.
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
                eprintln!("Failed to find note with uuid4 {}: {}", uuid4, e);
                return;
            }
        };

    // Then store the embedding
    let created_at = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO note_embedding (note_rowid, embedding, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![note_rowid, embedding_bytes, model_id, created_at],
    ) {
        eprintln!("Failed to store embedding: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Note;

    fn setup_test_db() -> Connection {
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
            created_at: chrono::Utc::now()
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
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
        insert(&conn, note);

        let notes = select::select_imp(&conn, &10, &0);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].uuid4, uuid);
        assert_eq!(notes[0].txt, "hello world");
    }

    #[test]
    fn test_insert_multiple_and_count() {
        let conn = setup_test_db();
        insert(&conn, make_test_note("note 1", "a"));
        insert(&conn, make_test_note("note 2", "b"));
        insert(&conn, make_test_note("note 3", "c"));
        assert_eq!(select::select_count(&conn), 3);
    }

    #[test]
    fn test_delete() {
        let conn = setup_test_db();
        insert(&conn, make_test_note("to delete", "x"));
        let notes = select::select_imp(&conn, &10, &0);
        assert_eq!(notes.len(), 1);
        delete(&conn, notes[0].rowid);
        assert_eq!(select::select_count(&conn), 0);
    }

    #[test]
    fn test_search() {
        let conn = setup_test_db();
        insert(&conn, make_test_note("rust programming", "code"));
        insert(&conn, make_test_note("python scripting", "code"));
        insert(&conn, make_test_note("cooking recipes", "food"));

        let result = search::search(&conn, "rust", &10, &0);
        let notes: Vec<Note> = serde_json::from_str(&result).unwrap();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].txt, "rust programming");
    }

    #[test]
    fn test_search_count() {
        let conn = setup_test_db();
        insert(&conn, make_test_note("rust lang", "code"));
        insert(&conn, make_test_note("rust book", "code"));
        insert(&conn, make_test_note("python book", "code"));
        assert_eq!(search::search_count(&conn, "rust"), 2);
        assert_eq!(search::search_count(&conn, "python"), 1);
        assert_eq!(search::search_count(&conn, "book"), 2);
    }

    #[test]
    fn test_search_empty_query_returns_all() {
        let conn = setup_test_db();
        insert(&conn, make_test_note("note 1", "a"));
        insert(&conn, make_test_note("note 2", "b"));
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
        insert(&conn, note);

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
        insert(&conn, updated);
        assert_eq!(select::select_count(&conn), 1);
        let notes = select::select_imp(&conn, &10, &0);
        assert_eq!(notes[0].txt, "updated");
    }

    #[test]
    fn test_select_pagination() {
        let conn = setup_test_db();
        for i in 0..5 {
            insert(&conn, make_test_note(&format!("note {}", i), "tag"));
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
        insert(&conn, note);
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
        insert(&conn, n1);
        insert(&conn, n2);

        let uuid3 = uuid::Uuid::new_v4().to_string();
        let missing =
            sync::diff_uuid4_to_server(&conn, vec![uuid1.clone(), uuid3.clone()]);
        assert_eq!(missing, vec![uuid3]);

        let from = sync::diff_uuid4_from_server(&conn, vec![uuid1]);
        assert_eq!(from, vec![uuid2]);
    }

    #[test]
    fn test_store_and_get_embedding() {
        let conn = setup_test_db();
        insert(&conn, make_test_note("embed me", "test"));
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
        insert(&conn, make_test_note("rust code", "code"));
        insert(&conn, make_test_note("python code", "code"));
        insert(&conn, make_test_note("recipe", "food"));

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
}
