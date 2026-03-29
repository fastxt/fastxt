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
use super::make_tags;
use super::select::{select, select_count};
use crate::Note;
use regex::Regex;
use rusqlite::Connection;
use rusqlite::types::ToSql;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Count notes matching `query` (space-separated keywords, all must match).
/// An empty query returns the total note count.
pub fn search_count(conn: &Connection, query: &str) -> u32 {
    let words = make_words(query);
    if words.len() == 1 && words[0].is_empty() {
        return select_count(conn);
    }
    let num_words = words.len();
    debug!(num_words, ?words, "search_count");

    let r: Vec<String> = where_vec(num_words);
    let sql = format!(
        "SELECT count(1)
        FROM note where
        {}",
        r.join(" and ")
    );

    debug!(sql = %sql, "search_count SQL");

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare search_count");
            return 0;
        }
    };
    let keys: Vec<String> = make_keys(num_words);

    let mut params: Vec<(&str, &dyn ToSql)> = vec![];
    for i in 0..num_words {
        params.push((&keys[i], &words[i] as &dyn ToSql));
    }

    debug!(num_params = params.len(), "search_count params");

    let rs = match stmt.query_map(&*params, |row| row.get(0)) {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "failed to query search_count");
            return 0;
        }
    };
    let mut c: u32 = 0;
    for r in rs.flatten() {
        c = r;
    }
    c
}

/// Search notes by keyword and return a JSON array of matching [`Note`]s.
/// Each word in `query` must appear in `txt` or `tags` (LIKE match).
/// An empty query falls back to a paginated `select`.
pub fn search(conn: &Connection, query: &str, limit: &u32, offset: &u32) -> String {
    let words = make_words(query);
    if words.len() == 1 && words[0].is_empty() {
        return select(conn, limit, offset);
    }
    let num_words = words.len();
    debug!(num_words, ?words, "search");

    let r: Vec<String> = where_vec(num_words);
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at, json(ai_tags), ai_summary, ai_category
        FROM note where
        {}
        order by created_at desc limit :limit offset :offset",
        r.join(" and ")
    );

    debug!(sql = %sql, "search SQL");

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare search");
            return "[]".to_string();
        }
    };
    let keys: Vec<String> = make_keys(num_words);

    let mut params: Vec<(&str, &dyn ToSql)> = vec![
        (":limit", limit as &dyn ToSql),
        (":offset", offset as &dyn ToSql),
    ];

    for i in 0..num_words {
        params.push((&keys[i], &words[i] as &dyn ToSql));
    }

    debug!(num_params = params.len(), "search params");

    let note_iter = match stmt.query_map(&*params, |row| {
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
    }) {
        Ok(iter) => iter,
        Err(e) => {
            warn!(error = %e, "failed to query search");
            return "[]".to_string();
        }
    };

    let notes: Vec<Note> = note_iter
        .filter_map(std::result::Result::ok)
        .map(|mut note| {
            note.tags = make_tags(&note.tags);
            note
        })
        .collect();

    serde_json::to_string(&notes).unwrap_or_else(|_| "[]".to_string())
}

fn make_words(query: &str) -> Vec<String> {
    use std::sync::LazyLock;
    static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let s1 = RE_SPACES.replace_all(query, " ");
    s1.trim()
        .split(' ')
        .map(|w| format!("%{w}%"))
        .collect::<Vec<String>>()
}

fn make_keys(num_words: usize) -> Vec<String> {
    (0..num_words).map(|i| format!(":w{i}")).collect()
}

fn where_vec(num_words: usize) -> Vec<String> {
    (0..num_words)
        .map(|i| {
            format!(
                "(
        txt like :w{i}
        or tags like :w{i}
        )"
            )
        })
        .collect()
}

/// A scored note from hybrid search, combining FTS5 and vector similarity.
#[derive(Debug, Clone)]
pub struct HybridSearchResult {
    pub note: Note,
    pub score: f64,
}

/// Hybrid search combining FTS5 full-text search and sqlite-vec vector
/// similarity using Reciprocal Rank Fusion (RRF).
///
/// - If FTS5 is available, runs a MATCH query for keyword relevance.
/// - If `query_embedding` is provided, runs a sqlite-vec nearest-neighbor query.
/// - Merges results using RRF: `score = sum(1.0 / (k + rank))` where `k = 60`.
/// - Falls back gracefully: FTS5 unavailable -> LIKE search; no embeddings -> skip vector.
pub fn hybrid_search(
    conn: &Connection,
    query: &str,
    query_embedding: Option<&[f32]>,
    limit: i64,
) -> Vec<HybridSearchResult> {
    const RRF_K: f64 = 60.0;
    let fetch_limit = limit * 3;

    // A map from note rowid to its RRF score accumulator.
    let mut scores: HashMap<i64, f64> = HashMap::new();

    // Step 1: FTS5 keyword search (or LIKE fallback).
    let fts_rowids = fts5_search(conn, query, fetch_limit)
        .or_else(|| {
            debug!("FTS5 not available, falling back to LIKE search");
            Some(like_search_rowids(conn, query, fetch_limit))
        })
        .unwrap_or_default();

    for (rank, rowid) in fts_rowids.iter().enumerate() {
        *scores.entry(*rowid).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    // Step 2: Vector search (if embedding provided).
    if let Some(embedding) = query_embedding {
        let vec_rowids = vec_search(conn, embedding, fetch_limit);
        for (rank, rowid) in vec_rowids.iter().enumerate() {
            *scores.entry(*rowid).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
        }
    }

    if scores.is_empty() {
        return Vec::new();
    }

    // Step 3: Sort by RRF score descending, take top `limit`.
    let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(limit as usize);

    // Step 4: Fetch full Note objects.
    let rowids: Vec<i64> = ranked.iter().map(|(r, _)| *r).collect();
    let notes = fetch_notes_by_rowids(conn, &rowids);

    ranked
        .into_iter()
        .filter_map(|(rowid, score)| {
            notes.get(&rowid).map(|note| HybridSearchResult {
                note: note.clone(),
                score,
            })
        })
        .collect()
}

/// Run FTS5 MATCH search, returning ordered rowids.
/// Returns `None` if the FTS5 table does not exist.
fn fts5_search(conn: &Connection, query: &str, limit: i64) -> Option<Vec<i64>> {
    if query.trim().is_empty() {
        return Some(Vec::new());
    }

    let fts_query = make_fts5_query(query);
    if fts_query.is_empty() {
        return Some(Vec::new());
    }

    let mut stmt = match conn
        .prepare("SELECT rowid, rank FROM note_fts WHERE note_fts MATCH ?1 ORDER BY rank LIMIT ?2")
    {
        Ok(s) => s,
        Err(e) => {
            debug!(error = %e, "FTS5 search not available");
            return None;
        }
    };

    let rows: Vec<i64> = match stmt.query_map(rusqlite::params![fts_query, limit], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            warn!(error = %e, "FTS5 query failed");
            return None;
        }
    };

    info!(count = rows.len(), "FTS5 search results");
    Some(rows)
}

/// Build an FTS5 query from a user query string.
/// Each word is quoted and joined with AND.
fn make_fts5_query(query: &str) -> String {
    use std::sync::LazyLock;
    static RE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

    let normalized = RE_SPACES.replace_all(query.trim(), " ");
    let words: Vec<String> = normalized
        .split(' ')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let escaped = w.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect();

    words.join(" AND ")
}

/// Fallback LIKE-based search returning ordered rowids.
fn like_search_rowids(conn: &Connection, query: &str, limit: i64) -> Vec<i64> {
    let words = make_words(query);
    if words.len() == 1 && words[0].is_empty() {
        return Vec::new();
    }
    let num_words = words.len();
    let r: Vec<String> = where_vec(num_words);
    let sql = format!(
        "SELECT rowid FROM note WHERE {} ORDER BY created_at DESC LIMIT :limit",
        r.join(" AND ")
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "LIKE fallback search failed to prepare");
            return Vec::new();
        }
    };

    let keys: Vec<String> = make_keys(num_words);
    let mut params: Vec<(&str, &dyn ToSql)> = vec![(":limit", &limit as &dyn ToSql)];
    for i in 0..num_words {
        params.push((&keys[i], &words[i] as &dyn ToSql));
    }

    match stmt.query_map(&*params, |row| row.get::<_, i64>(0)) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            warn!(error = %e, "LIKE fallback search query failed");
            Vec::new()
        }
    }
}

/// Run sqlite-vec nearest-neighbor search, returning ordered rowids.
fn vec_search(conn: &Connection, embedding: &[f32], limit: i64) -> Vec<i64> {
    let query_bytes = zerocopy::IntoBytes::as_bytes(embedding);

    let mut stmt = match conn.prepare(
        "SELECT note_rowid FROM vec_notes WHERE embedding MATCH ?1 ORDER BY distance LIMIT ?2",
    ) {
        Ok(s) => s,
        Err(e) => {
            debug!(error = %e, "vec_notes search not available");
            return Vec::new();
        }
    };

    match stmt.query_map(rusqlite::params![query_bytes, limit], |row| {
        row.get::<_, i64>(0)
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            warn!(error = %e, "vec_notes search query failed");
            Vec::new()
        }
    }
}

/// Fetch full Note objects for a set of rowids, returned as a map.
fn fetch_notes_by_rowids(conn: &Connection, rowids: &[i64]) -> HashMap<i64, Note> {
    if rowids.is_empty() {
        return HashMap::new();
    }

    let placeholders: String = rowids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at, json(ai_tags), ai_summary, ai_category
         FROM note WHERE rowid IN ({placeholders})"
    );

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to fetch notes by rowids");
            return HashMap::new();
        }
    };

    let params: Vec<&dyn rusqlite::ToSql> =
        rowids.iter().map(|r| r as &dyn rusqlite::ToSql).collect();

    match stmt.query_map(params.as_slice(), |row| {
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
    }) {
        Ok(rows) => rows
            .filter_map(|r| r.ok())
            .map(|mut note| {
                note.tags = make_tags(&note.tags);
                (note.rowid, note)
            })
            .collect(),
        Err(e) => {
            warn!(error = %e, "failed to query notes by rowids");
            HashMap::new()
        }
    }
}
