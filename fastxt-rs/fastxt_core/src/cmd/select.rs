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
use rusqlite::{Connection, NO_PARAMS};

pub fn select(conn: &Connection, limit: &u32, offset: &u32) -> Vec<Note> {
    let mut stmt = conn
        .prepare(
            "SELECT rowid, uuid4, txt, tags, created_at
        FROM note
        order by created_at desc limit :limit offset :offset",
        )
        .unwrap();
    let note_iter = stmt
        .query_map_named(
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
