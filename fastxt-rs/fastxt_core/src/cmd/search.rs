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
use rusqlite::types::ToSql;
use rusqlite::Connection;

pub fn search_count(conn: &Connection, query: &str) -> u32 {
    let words = make_words(query);
    if words.len() == 1 && words[0].is_empty() {
        return select_count(conn);
    }
    let num_words = words.len();
    eprintln!("{} words {:?}", num_words, words);

    let r: Vec<String> = where_vec(num_words);
    let sql = format!(
        "SELECT count(1)
        FROM note where
        {}",
        r.join(" and ")
    );

    eprintln!("sql {}", sql);

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to prepare search_count: {}", e);
            return 0;
        }
    };
    let keys: Vec<String> = make_keys(num_words);

    let mut params: Vec<(&str, &dyn ToSql)> = vec![];
    for i in 0..num_words {
        params.push((&keys[i], &words[i] as &dyn ToSql));
    }

    eprintln!("params {:?}", params.len());

    let rs = match stmt.query_map(&*params, |row| row.get(0)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to query search_count: {}", e);
            return 0;
        }
    };
    let mut c: u32 = 0;
    for r in rs.flatten() {
        c = r;
    }
    c
}

pub fn search(conn: &Connection, query: &str, limit: &u32, offset: &u32) -> String {
    let words = make_words(query);
    if words.len() == 1 && words[0].is_empty() {
        return select(conn, limit, offset);
    }
    let num_words = words.len();
    eprintln!("{} words {:?}", num_words, words);

    let r: Vec<String> = where_vec(num_words);
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at, ai_tags, ai_summary, ai_category
        FROM note where
        {}
        order by created_at desc limit :limit offset :offset",
        r.join(" and ")
    );

    eprintln!("sql {}", sql);

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to prepare search: {}", e);
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

    eprintln!("params {:?}", params.len());

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
            eprintln!("Failed to query search: {}", e);
            return "[]".to_string();
        }
    };

    let notes: Vec<Note> = note_iter
        .filter_map(|r| r.ok())
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
        .split(" ")
        .map(|w| format!("%{}%", w))
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
