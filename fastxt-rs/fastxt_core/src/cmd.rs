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

pub mod select;
use rusqlite::{Connection, NO_PARAMS};
pub fn create(conn: &Connection) {
    conn.execute_batch(
        "BEGIN;
        CREATE TABLE IF NOT EXISTS note (
         rowid          INTEGER PRIMARY KEY AUTOINCREMENT,
         uuid4          TEXT NOT NULL UNIQUE,
         txt            TEXT NOT NULL,
         tags           TEXT NOT NULL,
         created_at     TIMESTAMP  DEFAULT CURRENT_TIMESTAMP
         );

         CREATE TABLE IF NOT EXISTS meta (
         meta_key        TEXT PRIMARY KEY,
         meta_value      TEXT NOT NULL
         );
         COMMIT;",
    )
    .unwrap();
}
