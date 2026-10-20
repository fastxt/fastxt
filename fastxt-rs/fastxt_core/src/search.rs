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

//! Note listing and search.
//!
//! Text search uses the FTS5 trigram index (which matches inside words and
//! inside CJK text). Trigram indexes need terms of at least 3 characters, so
//! queries with shorter terms fall back to LIKE. Listing and search both take
//! a [`Filter`] (AI category, user tags) that is applied in SQL, so counts
//! and pages stay right. Hybrid search fuses full-text rank and vector
//! similarity with Reciprocal Rank Fusion.

use crate::error::Result;
use crate::model::{Filter, Note, Page, ScoredNote};
use crate::store::{Fastxt, NOTE_COLUMNS, note_from_row};
use rusqlite::types::ToSql;
use rusqlite::{Connection, params};
use std::collections::{HashMap, HashSet};

/// Minimum term length the trigram index can match; shorter terms use LIKE.
const TRIGRAM_MIN: usize = 3;

/// Split a query into search terms (whitespace separated, deduplicated).
#[must_use]
pub fn terms(query: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for term in query.split_whitespace() {
        if !term.is_empty() && seen.insert(term.to_lowercase()) {
            out.push(term.to_string());
        }
    }
    out
}

fn fts_match_expr(q: &[String]) -> String {
    q.iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Escape `%`, `_` and `\` so a LIKE pattern matches `text` literally.
fn like_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// SQL for a [`Filter`]: a condition on the `note n` row (never empty) and
/// its parameter values, numbered `?{first}` onwards.
fn filter_sql(filter: &Filter, first: usize) -> (String, Vec<String>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut values: Vec<String> = Vec::new();
    if let Some(category) = &filter.category {
        values.push(category.clone());
        clauses.push(format!("n.ai_category = ?{}", first + values.len() - 1));
    }
    for tag in &filter.tags {
        let tag = crate::tags::clean(tag);
        if tag.is_empty() {
            continue;
        }
        // Tags are stored comma-joined, so wrapping both sides in commas makes
        // membership an exact (ASCII case-insensitive) LIKE match: `go` does
        // not match `golang`.
        values.push(format!("%,{},%", like_escape(&tag)));
        clauses.push(format!(
            "(',' || n.tags || ',') LIKE ?{} ESCAPE '\\'",
            first + values.len() - 1
        ));
    }
    if clauses.is_empty() {
        clauses.push("1 = 1".into());
    }
    (clauses.join(" AND "), values)
}

fn fetch_page(
    conn: &Connection,
    where_clause: &str,
    params: &[&dyn ToSql],
    limit: u32,
    offset: u32,
) -> Result<Page> {
    let count: u32 = conn.query_row(
        &format!("SELECT count(*) FROM note n WHERE n.deleted = 0 AND ({where_clause})"),
        params,
        |r| r.get(0),
    )?;
    if count == 0 || offset >= count {
        return Ok(Page {
            count,
            notes: Vec::new(),
        });
    }
    let mut all: Vec<&dyn ToSql> = params.to_vec();
    all.push(&limit);
    all.push(&offset);
    let mut stmt = conn.prepare(&format!(
        "SELECT {NOTE_COLUMNS} FROM note n WHERE n.deleted = 0 AND ({where_clause})
         ORDER BY n.created_at DESC, n.rowid DESC LIMIT ? OFFSET ?"
    ))?;
    let notes = stmt
        .query_map(all.as_slice(), note_from_row)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Page { count, notes })
}

impl Fastxt {
    /// List the newest notes, optionally filtered.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn list(&self, limit: u32, offset: u32, filter: &Filter) -> Result<Page> {
        let (clause, values) = filter_sql(filter, 1);
        let params: Vec<&dyn ToSql> = values.iter().map(|v| v as &dyn ToSql).collect();
        fetch_page(&self.conn, &clause, &params, limit, offset)
    }

    /// Search text, user tags and AI tags. Every term must match, and so must
    /// `filter`. An empty query lists notes, as [`Fastxt::list`] does.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn search(&self, query: &str, limit: u32, offset: u32, filter: &Filter) -> Result<Page> {
        let q = terms(query);
        if q.is_empty() {
            return self.list(limit, offset, filter);
        }
        if q.iter().all(|t| t.chars().count() >= TRIGRAM_MIN) {
            let expr = fts_match_expr(&q);
            if let Some(page) = self.fts_page(&expr, filter, limit, offset)? {
                return Ok(page);
            }
        }
        self.like_page(&q, filter, limit, offset)
    }

    /// FTS search over the trigram index; `None` means the index is unusable.
    fn fts_page(
        &self,
        expr: &str,
        filter: &Filter,
        limit: u32,
        offset: u32,
    ) -> Result<Option<Page>> {
        let (clause, values) = filter_sql(filter, 2);
        let condition = format!("note_fts MATCH ?1 AND n.deleted = 0 AND ({clause})");
        let mut params: Vec<&dyn ToSql> = Vec::with_capacity(values.len() + 3);
        params.push(&expr);
        params.extend(values.iter().map(|v| v as &dyn ToSql));
        let Ok(count) = self.conn.query_row(
            &format!(
                "SELECT count(*) FROM note_fts f JOIN note n ON n.rowid = f.rowid
                 WHERE {condition}"
            ),
            params.as_slice(),
            |r| r.get::<_, u32>(0),
        ) else {
            return Ok(None);
        };
        if count == 0 || offset >= count {
            return Ok(Some(Page {
                count,
                notes: Vec::new(),
            }));
        }
        params.push(&limit);
        params.push(&offset);
        let mut stmt = match self.conn.prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM note_fts f JOIN note n ON n.rowid = f.rowid
             WHERE {condition} ORDER BY f.rank LIMIT ? OFFSET ?"
        )) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        match stmt.query_map(params.as_slice(), note_from_row) {
            Ok(rows) => {
                let notes = rows.collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(Some(Page { count, notes }))
            }
            Err(_) => Ok(None),
        }
    }

    /// LIKE fallback for terms too short for the trigram index.
    /// `%`, `_` and `\` are escaped so terms match literally.
    fn like_page(&self, q: &[String], filter: &Filter, limit: u32, offset: u32) -> Result<Page> {
        let values = like_values(q);
        let (filter_clause, filter_values) = filter_sql(filter, q.len() + 1);
        let clause = format!("{} AND ({filter_clause})", like_clause(q.len()));
        let params: Vec<&dyn ToSql> = values
            .iter()
            .chain(filter_values.iter())
            .map(|v| v as &dyn ToSql)
            .collect();
        fetch_page(&self.conn, &clause, &params, limit, offset)
    }

    /// Hybrid search: full-text rank fused with vector similarity (RRF,
    /// k = 60). `embedding` is the query vector and the model it came from.
    /// Works with either half alone.
    ///
    /// # Errors
    /// Fails on a database or embedding error.
    pub fn hybrid_search(
        &self,
        query: &str,
        embedding: Option<(&[f32], &str)>,
        limit: u32,
    ) -> Result<Vec<ScoredNote>> {
        const RRF_K: f64 = 60.0;
        let fetch = (limit.saturating_mul(4)).max(20) as usize;
        let mut scores: HashMap<i64, f64> = HashMap::new();

        let q = terms(query);
        if !q.is_empty() && q.iter().all(|t| t.chars().count() >= TRIGRAM_MIN) {
            let expr = fts_match_expr(&q);
            if let Ok(mut stmt) = self.conn.prepare(
                "SELECT f.rowid FROM note_fts f JOIN note n ON n.rowid = f.rowid
                 WHERE note_fts MATCH ?1 AND n.deleted = 0 ORDER BY f.rank LIMIT ?2",
            ) && let Ok(rows) =
                stmt.query_map(params![expr, fetch as i64], |r| r.get::<_, i64>(0))
            {
                for (rank, rowid) in rows.flatten().enumerate() {
                    *scores.entry(rowid).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
                }
            }
        } else if !q.is_empty() {
            for (rank, rowid) in self.like_rowids(&q, fetch as i64).into_iter().enumerate() {
                *scores.entry(rowid).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
        }

        if let Some((vector, model_id)) = embedding {
            for (rank, hit) in self
                .semantic_search(vector, model_id, fetch as u32, f64::NEG_INFINITY)?
                .into_iter()
                .enumerate()
            {
                *scores.entry(hit.note.rowid).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
        }

        let mut ranked: Vec<(i64, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        ranked.truncate(limit as usize);
        if ranked.is_empty() {
            return Ok(Vec::new());
        }

        let ids: Vec<i64> = ranked.iter().map(|(r, _)| *r).collect();
        let placeholders = vec!["?"; ids.len()].join(",");
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM note n WHERE n.rowid IN ({placeholders})"
        ))?;
        let id_params: Vec<&dyn ToSql> = ids.iter().map(|r| r as &dyn ToSql).collect();
        let notes: HashMap<i64, Note> = stmt
            .query_map(id_params.as_slice(), |row| {
                Ok((row.get::<_, i64>(0)?, note_from_row(row)?))
            })?
            .collect::<rusqlite::Result<HashMap<_, _>>>()?;
        Ok(ranked
            .into_iter()
            .filter_map(|(rowid, score)| {
                notes.get(&rowid).map(|note| ScoredNote {
                    note: note.clone(),
                    score,
                })
            })
            .collect())
    }

    fn like_rowids(&self, q: &[String], limit: i64) -> Vec<i64> {
        let values = like_values(q);
        let mut params: Vec<&dyn ToSql> = values.iter().map(|v| v as &dyn ToSql).collect();
        params.push(&limit);
        let Ok(mut stmt) = self.conn.prepare(&format!(
            "SELECT n.rowid FROM note n WHERE n.deleted = 0 AND {}
             ORDER BY n.created_at DESC LIMIT ?{}",
            like_clause(q.len()),
            q.len() + 1
        )) else {
            return Vec::new();
        };
        match stmt.query_map(params.as_slice(), |r| r.get::<_, i64>(0)) {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }
}

/// `LIKE` conditions with literal term matching (`?1`…`?n`), joined by AND.
fn like_clause(num_terms: usize) -> String {
    (1..=num_terms)
        .map(|i| {
            format!(
                "(n.txt LIKE ?{i} ESCAPE '\\' OR n.tags LIKE ?{i} ESCAPE '\\'
                  OR n.ai_tags LIKE ?{i} ESCAPE '\\')"
            )
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Wrapped, escaped parameter values for [`like_clause`].
fn like_values(q: &[String]) -> Vec<String> {
    q.iter()
        .map(|term| format!("%{}%", like_escape(term)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NewNote, NoteKey};

    fn db() -> Fastxt {
        Fastxt::open_in_memory().unwrap()
    }

    fn add(db: &mut Fastxt, txt: &str, tags: &str, category: Option<&str>) {
        let n = db.insert(NewNote::new(txt, tags)).unwrap();
        if let Some(c) = category {
            db.set_ai_category(&NoteKey::Rowid(n.rowid), Some(c))
                .unwrap();
        }
    }

    /// Matches for `query` with no filter.
    fn hits(db: &Fastxt, query: &str) -> u32 {
        db.search(query, 10, 0, &Filter::default()).unwrap().count
    }

    fn tagged(tags: &[&str]) -> Filter {
        Filter {
            tags: tags.iter().map(|t| (*t).to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_query_lists_all_newest_first_with_count() {
        let mut db = db();
        add(&mut db, "one", "", None);
        add(&mut db, "two", "", None);
        let page = db.search("", 1, 0, &Filter::default()).unwrap();
        assert_eq!(page.count, 2);
        assert_eq!(page.notes.len(), 1);
        assert_eq!(page.notes[0].txt, "two");
        let page2 = db.list(10, 0, &Filter::default()).unwrap();
        assert_eq!(page2.count, 2);
    }

    #[test]
    fn fts_matches_substrings_and_cjk() {
        let mut db = db();
        add(&mut db, "今天学习了向量数据库的用法", "学习", None);
        add(&mut db, "rust programming notes", "code", None);
        add(&mut db, "unrelated", "", None);

        assert_eq!(hits(&db, "向量数据"), 1);
        assert_eq!(hits(&db, "向量"), 1, "CJK term");
        assert_eq!(hits(&db, "gramm"), 1, "substring");
        assert_eq!(hits(&db, "rust 学习"), 0, "AND terms");
    }

    #[test]
    fn short_terms_fall_back_to_like() {
        let mut db = db();
        add(&mut db, "db design notes", "go", None);
        add(&mut db, "other", "", None);
        assert_eq!(hits(&db, "db"), 1);
        assert_eq!(hits(&db, "go"), 1, "tag match");
        assert_eq!(hits(&db, "zz"), 0);
    }

    #[test]
    fn ai_tags_are_searched() {
        let mut db = db();
        let n = db
            .insert(NewNote::new("a note about databases", ""))
            .unwrap();
        db.set_ai_tags(&NoteKey::Rowid(n.rowid), &["database".into()])
            .unwrap();
        assert_eq!(hits(&db, "database"), 1);
    }

    #[test]
    fn search_ignores_deleted_notes() {
        let mut db = db();
        let n = db.insert(NewNote::new("unique needle", "")).unwrap();
        assert_eq!(hits(&db, "needle"), 1);
        db.delete(&NoteKey::Rowid(n.rowid)).unwrap();
        assert_eq!(hits(&db, "needle"), 0);
    }

    #[test]
    fn category_filter() {
        let mut db = db();
        add(&mut db, "a", "", Some("work"));
        add(&mut db, "b", "", Some("home"));
        add(&mut db, "c", "", None);
        let work = Filter {
            category: Some("work".into()),
            ..Default::default()
        };
        let page = db.list(10, 0, &work).unwrap();
        assert_eq!(page.count, 1);
        assert_eq!(page.notes[0].txt, "a");
    }

    #[test]
    fn tag_filter_matches_whole_tags_on_every_path() {
        let mut db = db();
        add(&mut db, "alpha note", "go, topic:rust async", None);
        add(&mut db, "beta note", "golang", Some("work"));
        add(&mut db, "gamma note", "Go", None);

        // Listing: whole tags, case-insensitive, no substring matches.
        let page = db.list(10, 0, &tagged(&["go"])).unwrap();
        assert_eq!(page.count, 2);
        assert!(page.notes.iter().all(|n| n.txt != "beta note"));
        assert_eq!(
            db.list(10, 0, &tagged(&["topic:rust async"]))
                .unwrap()
                .count,
            1
        );
        // Every listed tag must be present.
        assert_eq!(
            db.list(10, 0, &tagged(&["go", "topic:rust async"]))
                .unwrap()
                .count,
            1
        );
        assert_eq!(db.list(10, 0, &tagged(&["go", "golang"])).unwrap().count, 0);
        // Combined with a category.
        let mut both = tagged(&["golang"]);
        both.category = Some("work".into());
        assert_eq!(db.list(10, 0, &both).unwrap().count, 1);
        both.category = Some("home".into());
        assert_eq!(db.list(10, 0, &both).unwrap().count, 0);

        // The same filter on the FTS path (long term) and the LIKE path.
        assert_eq!(db.search("note", 10, 0, &tagged(&["go"])).unwrap().count, 2);
        assert_eq!(db.search("no", 10, 0, &tagged(&["go"])).unwrap().count, 2);
        assert_eq!(db.search("beta", 10, 0, &tagged(&["go"])).unwrap().count, 0);
        assert_eq!(db.search("be", 10, 0, &tagged(&["go"])).unwrap().count, 0);

        // LIKE metacharacters inside a tag are literal; blanks are ignored.
        add(&mut db, "delta", "100%_done", None);
        assert_eq!(db.list(10, 0, &tagged(&["100%_done"])).unwrap().count, 1);
        assert_eq!(db.list(10, 0, &tagged(&["100_done"])).unwrap().count, 0);
        assert_eq!(db.list(10, 0, &tagged(&["  "])).unwrap().count, 4);
    }

    #[test]
    fn hybrid_fuses_text_and_vector_hits() {
        let mut db = db();
        let alpha = db
            .insert(NewNote::new("alpha note about databases", ""))
            .unwrap();
        let beta = db
            .insert(NewNote::new("beta note about cooking", ""))
            .unwrap();
        db.store_embedding(&NoteKey::Rowid(alpha.rowid), "m", &[1.0, 0.0])
            .unwrap();
        db.store_embedding(&NoteKey::Rowid(beta.rowid), "m", &[0.0, 1.0])
            .unwrap();

        // Text hit only.
        let r = db.hybrid_search("databases", None, 10).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].note.txt, "alpha note about databases");

        // Vector hit only (query text matches nothing).
        let r = db
            .hybrid_search("zzzz", Some((&[1.0, 0.0], "m")), 10)
            .unwrap();
        assert_eq!(r[0].note.txt, "alpha note about databases");

        // Both halves: the note found by both outranks a vector-only match.
        let r = db
            .hybrid_search("databases", Some((&[1.0, 0.1], "m")), 10)
            .unwrap();
        assert_eq!(r[0].note.txt, "alpha note about databases");
        assert!(r[0].score > r[1].score);
    }

    #[test]
    fn special_characters_do_not_break_queries() {
        let mut db = db();
        add(&mut db, "50% of 100_users", "", None);
        assert_eq!(hits(&db, "100_users"), 1);
        assert_eq!(hits(&db, "50%"), 1);
    }
}
