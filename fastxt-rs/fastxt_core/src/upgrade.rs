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

use semver::Version;
use rusqlite::Connection;
// version to upgrade to
const VERSION: &str = "0.2.0";
use crate::OneString;

fn set_meta_version(conn: &Connection, version: &str) {
    if let Err(e) = conn.execute(
        "
        UPDATE meta SET meta_value = ?1
        WHERE meta_key = 'version';",
        [version],
    ) {
        eprintln!("Failed to update meta version: {}", e);
    }
}

pub fn upgrade(conn: &Connection) -> Result<&str, &str> {
    if get_meta_is_upgrading(conn) {
        eprintln!("is_upgrading");
        Err("is_upgrading")
    } else {
        let current = Version::parse(&get_meta_version(conn)).ok();
        let v0_1_0 = Version::parse("0.1.0").ok();
        let v0_2_0 = Version::parse("0.2.0").ok();

        // Migration to 0.1.0
        if current < v0_1_0 {
            set_meta_version(conn, "0.1.0");
            eprintln!("upgraded to 0.1.0")
        }

        // Migration to 0.2.0 - Add AI columns
        if current < v0_2_0 {
            crate::cmd::migrate_ai_columns(conn);
            set_meta_version(conn, "0.2.0");
            eprintln!("upgraded to 0.2.0 (added AI columns)")
        }

        let updated = Version::parse(&get_meta_version(conn)).ok();
        if updated == v0_2_0 {
            set_meta_version(conn, VERSION);
        }
        eprintln!("upgraded to {}", VERSION);
        Ok(VERSION)
    }
}

fn get_meta_is_upgrading(conn: &Connection) -> bool {
    let mut stmt = match conn
        .prepare("SELECT meta_value FROM meta where meta_key = 'is_upgrading' ")
    {
        Ok(s) => s,
        Err(_) => return false,
    };
    match stmt.query_row([], |row| Ok(OneString { s: row.get(0)? })) {
        Ok(is_upgrading) => {
            if is_upgrading.s == "1" {
                eprintln!("get_meta_is_upgrading: true");
                true
            } else {
                eprintln!("get_meta_is_upgrading: false");
                false
            }
        }
        Err(_) => false,
    }
}

pub fn get_meta_version(conn: &Connection) -> String {
    let mut stmt = match conn
        .prepare("SELECT meta_value FROM meta where meta_key = 'version' ")
    {
        Ok(s) => s,
        Err(_) => return "0.0.0".to_string(),
    };
    match stmt.query_row([], |row| Ok(OneString { s: row.get(0)? })) {
        Ok(version) => {
            eprintln!("get_meta_version {}", version.s);
            version.s
        }
        Err(_) => {
            if let Err(e) = conn.execute_batch(
                "
            INSERT INTO meta
            (meta_key, meta_value)
            VALUES
            ('version', '0.0.0')
            ;",
            ) {
                eprintln!("Failed to initialize meta version: {}", e);
            }
            eprintln!("get_meta_version: version init to 0.0.0");
            "0.0.0".to_string()
        }
    }
}
