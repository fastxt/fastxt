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
use rusqlite::types::ToSql;
use rusqlite::Connection;

pub fn select_count(conn: &Connection) -> u32 {
    let mut stmt = conn.prepare("SELECT count(1) FROM note").unwrap();
    stmt.query_row([], |row| row.get(0)).unwrap()
}

pub fn select(conn: &Connection, limit: &u32, offset: &u32) -> String {
    let result_iter = select_imp(conn, limit, offset);
    let mut d = "[ ".to_owned();
    for r in result_iter {
        d.push_str(&serde_json::to_string(&r).unwrap());
        d.push(',');
    }
    d.pop();
    d.push(']');
    d
}

pub fn select_imp(conn: &Connection, limit: &u32, offset: &u32) -> Vec<Note> {
    let mut stmt = conn
        .prepare(
            "SELECT rowid, uuid4, txt, tags, created_at, ai_tags, ai_summary, ai_category
            FROM note
            order by created_at desc limit :limit offset :offset",
        )
        .unwrap();

    let note_iter = stmt
        .query_map(
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
        )
        .unwrap();

    let mut result = Vec::new();
    for name_result in note_iter {
        result.push(name_result.unwrap());
    }

    result
}
