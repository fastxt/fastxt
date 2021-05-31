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
    if words.len() == 1 && words.get(0).unwrap().is_empty() {
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

    let mut stmt = conn.prepare(&sql).unwrap();
    let keys: Vec<String> = make_keys(num_words);

    let mut params: Vec<(&str, &dyn ToSql)> = vec![];
    for i in 0..num_words {
        params.push((&keys.get(i).unwrap(), words.get(i).unwrap() as &dyn ToSql));
    }

    eprintln!("params {:?}", params.len());

    let rs = stmt.query_map(&*params, |row| row.get(0)).unwrap();
    let mut c: u32 = 0;
    for r in rs {
        c = r.unwrap();
    }
    c
}

pub fn search(conn: &Connection, query: &str, limit: &u32, offset: &u32) -> String {
    let words = make_words(query);
    if words.len() == 1 && words.get(0).unwrap().is_empty() {
        return select(conn, limit, offset);
    }
    let num_words = words.len();
    eprintln!("{} words {:?}", num_words, words);

    let r: Vec<String> = where_vec(num_words);
    let sql = format!(
        "SELECT rowid, uuid4, txt, tags, created_at
        FROM note where
        {}
        order by created_at desc limit :limit offset :offset",
        r.join(" and ")
    );

    eprintln!("sql {}", sql);

    let mut stmt = conn.prepare(&sql).unwrap();
    let keys: Vec<String> = make_keys(num_words);

    let mut params: Vec<(&str, &dyn ToSql)> = vec![
        (":limit", limit as &dyn ToSql),
        (":offset", offset as &dyn ToSql),
    ];

    for i in 0..num_words {
        params.push((&keys.get(i).unwrap(), words.get(i).unwrap() as &dyn ToSql));
    }

    eprintln!("params {:?}", params.len());

    let note_iter = stmt
        .query_map(&*params, |row| {
            Ok(Note {
                rowid: row.get(0)?,
                uuid4: row.get(1)?,
                txt: row.get(2)?,
                tags: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .unwrap();

    let mut j = "[ ".to_owned();
    for note in note_iter {
        let mut note = note.unwrap();
        note.tags = make_tags(&note.tags);
        //eprintln!("Found note {:?}", note);
        j.push_str(&serde_json::to_string(&note).unwrap());
        j.push_str(",");
    }
    j.pop();
    j.push_str("]");
    j
}

fn make_words(query: &str) -> Vec<String> {
    let re1 = Regex::new(r"\s+").unwrap();
    let s1 = re1.replace_all(query, " ");
    s1.trim()
        .split(" ")
        .map(|w| format!("%{}%", w))
        .collect::<Vec<String>>()
}

fn make_keys(num_words: usize) -> Vec<String> {
    (0..num_words)
        .map(|i| ":w".to_string() + &i.to_string())
        .collect()
}

fn where_vec(num_words: usize) -> Vec<String> {
    (0..num_words)
        .map(|i| {
            format!(
                "(
        txt like :w{}
        or tags like :w{}
        )",
                i.to_string(),
                i.to_string()
            )
        })
        .collect()
}
