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

use rusqlite::Connection;
use semver::Version;
use tracing::{debug, info, warn};
// version to upgrade to
const VERSION: &str = "0.5.0";
use crate::OneString;

fn set_meta_version(conn: &Connection, version: &str) {
    if let Err(e) = conn.execute(
        "
        UPDATE meta SET meta_value = ?1
        WHERE meta_key = 'version';",
        [version],
    ) {
        warn!(error = %e, "failed to update meta version");
    }
}

/// Run all pending database migrations and update the stored schema version.
///
/// Returns `Ok(VERSION)` on success.
///
/// # Errors
/// Returns `Err("is_upgrading")` if another upgrade is already in progress.
pub fn upgrade(conn: &Connection) -> Result<&str, &str> {
    if get_meta_is_upgrading(conn) {
        warn!("database is currently upgrading");
        Err("is_upgrading")
    } else {
        let current = Version::parse(&get_meta_version(conn)).ok();
        let v0_1_0 = Version::parse("0.1.0").ok();
        let v0_2_0 = Version::parse("0.2.0").ok();
        let v0_3_0 = Version::parse("0.3.0").ok();
        let v0_4_0 = Version::parse("0.4.0").ok();
        let v0_5_0 = Version::parse("0.5.0").ok();

        // Migration to 0.1.0
        if current < v0_1_0 {
            set_meta_version(conn, "0.1.0");
            info!("upgraded to 0.1.0");
        }

        // Migration to 0.2.0 - Add AI columns
        if current < v0_2_0 {
            crate::cmd::migrate_ai_columns(conn);
            set_meta_version(conn, "0.2.0");
            info!("upgraded to 0.2.0 (added AI columns)");
        }

        // Migration to 0.3.0 - sqlite-vec virtual table for vector search
        if current < v0_3_0 {
            migrate_vec_notes(conn);
            set_meta_version(conn, "0.3.0");
            info!("upgraded to 0.3.0 (added sqlite-vec vector search)");
        }

        // Migration to 0.4.0 - Convert ai_tags from TEXT to JSONB
        if Version::parse(&get_meta_version(conn)).ok() < v0_4_0 {
            migrate_ai_tags_to_jsonb(conn);
            set_meta_version(conn, "0.4.0");
            info!("upgraded to 0.4.0 (converted ai_tags to JSONB)");
        }

        // Migration to 0.5.0 - FTS5 full-text search
        if Version::parse(&get_meta_version(conn)).ok() < v0_5_0 {
            crate::cmd::migrate_fts5(conn);
            set_meta_version(conn, "0.5.0");
            info!("upgraded to 0.5.0 (added FTS5 full-text search)");
        }

        let updated = Version::parse(&get_meta_version(conn)).ok();
        if updated == v0_5_0 {
            set_meta_version(conn, VERSION);
        }
        info!(version = VERSION, "upgrade complete");
        Ok(VERSION)
    }
}

fn get_meta_is_upgrading(conn: &Connection) -> bool {
    let Ok(mut stmt) = conn.prepare("SELECT meta_value FROM meta where meta_key = 'is_upgrading' ")
    else {
        return false;
    };
    let Ok(is_upgrading) = stmt.query_row([], |row| Ok(OneString { s: row.get(0)? })) else {
        return false;
    };
    if is_upgrading.s == "1" {
        debug!("get_meta_is_upgrading: true");
        true
    } else {
        debug!("get_meta_is_upgrading: false");
        false
    }
}

/// Read the current schema version from the `meta` table.
/// Inserts a `"0.0.0"` row if no version entry exists yet.
pub fn get_meta_version(conn: &Connection) -> String {
    let Ok(mut stmt) = conn.prepare("SELECT meta_value FROM meta where meta_key = 'version' ")
    else {
        return "0.0.0".to_string();
    };
    if let Ok(version) = stmt.query_row([], |row| Ok(OneString { s: row.get(0)? })) {
        debug!(version = %version.s, "get_meta_version");
        return version.s;
    }
    if let Err(e) = conn.execute_batch(
        "
    INSERT INTO meta
    (meta_key, meta_value)
    VALUES
    ('version', '0.0.0')
    ;",
    ) {
        warn!(error = %e, "failed to initialize meta version");
    }
    info!("meta version initialized to 0.0.0");
    "0.0.0".to_string()
}

/// Migrate existing embeddings from `note_embedding` into the `vec_notes`
/// sqlite-vec virtual table. The `note_embedding` table is kept for
/// `model_id` tracking (vec_notes does not store metadata).
fn migrate_vec_notes(conn: &Connection) {
    use crate::cmd::DEFAULT_EMBEDDING_DIM;

    // Ensure the vec_notes virtual table exists.
    crate::cmd::create_vec_table(conn, DEFAULT_EMBEDDING_DIM);

    // Count existing embeddings to migrate.
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM note_embedding", [], |row| row.get(0))
        .unwrap_or(0);

    if count == 0 {
        info!("no existing embeddings to migrate to vec_notes");
        return;
    }

    info!(count, "migrating existing embeddings to vec_notes");

    // Read all existing embeddings and insert into vec_notes.
    let mut stmt = match conn.prepare("SELECT note_rowid, embedding FROM note_embedding") {
        Ok(s) => s,
        Err(e) => {
            warn!(error = %e, "failed to prepare migration query");
            return;
        }
    };

    let rows: Vec<(i64, Vec<u8>)> = match stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    }) {
        Ok(r) => r.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            warn!(error = %e, "failed to query embeddings for migration");
            return;
        }
    };

    let mut migrated = 0u64;
    let mut skipped = 0u64;

    for (note_rowid, embedding_bytes) in &rows {
        // Verify the embedding dimension matches what vec_notes expects.
        let dim = embedding_bytes.len() / 4; // 4 bytes per f32
        if dim != DEFAULT_EMBEDDING_DIM {
            debug!(
                note_rowid,
                dim,
                expected = DEFAULT_EMBEDDING_DIM,
                "skipping embedding with mismatched dimension"
            );
            skipped += 1;
            continue;
        }

        // Delete any existing entry first (idempotent).
        let _ = conn.execute(
            "DELETE FROM vec_notes WHERE note_rowid = ?1",
            rusqlite::params![note_rowid],
        );
        if let Err(e) = conn.execute(
            "INSERT INTO vec_notes(note_rowid, embedding) VALUES (?1, ?2)",
            rusqlite::params![note_rowid, embedding_bytes],
        ) {
            warn!(note_rowid, error = %e, "failed to migrate embedding to vec_notes");
        } else {
            migrated += 1;
        }
    }

    info!(migrated, skipped, "vec_notes migration complete");
}

/// Convert existing TEXT ai_tags to JSONB format for more compact storage.
///
/// Uses SQLite's `jsonb()` function (available since SQLite 3.45.0).
/// Only updates rows where ai_tags is non-null and non-empty.
fn migrate_ai_tags_to_jsonb(conn: &Connection) {
    match conn.execute(
        "UPDATE note SET ai_tags = jsonb(ai_tags) WHERE ai_tags IS NOT NULL AND ai_tags != ''",
        [],
    ) {
        Ok(rows) => {
            info!(rows, "migrated ai_tags to JSONB format");
        }
        Err(e) => {
            warn!(error = %e, "failed to migrate ai_tags to JSONB (requires SQLite 3.45+)");
        }
    }
}
