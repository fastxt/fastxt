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

//! Database schema creation and migrations.
//!
//! Every migration runs inside one transaction and the schema version is only
//! bumped when all of it succeeded, so a failure leaves the database as it was.

use crate::clock::LEGACY_NODE;
use crate::error::{Error, Result};
use rusqlite::{Connection, OptionalExtension, Transaction};
use semver::Version;
use std::time::Duration;
use tracing::info;

/// Schema version written by this build. Independent of the sync protocol
/// version ([`crate::sync::PROTOCOL_VERSION`]).
pub const SCHEMA_VERSION: &str = "0.6.0";

const CREATE_META: &str = "CREATE TABLE IF NOT EXISTS meta (
    meta_key   TEXT PRIMARY KEY,
    meta_value TEXT NOT NULL
)";

const CREATE_NOTE: &str = "CREATE TABLE IF NOT EXISTS note (
    rowid         INTEGER PRIMARY KEY AUTOINCREMENT,
    uuid4         TEXT NOT NULL UNIQUE,
    txt           TEXT NOT NULL,
    tags          TEXT NOT NULL,
    created_at    TEXT NOT NULL,
    ai_tags       TEXT,
    ai_summary    TEXT,
    ai_category   TEXT,
    updated_at    TEXT NOT NULL DEFAULT '',
    ai_updated_at TEXT NOT NULL DEFAULT '',
    deleted       INTEGER NOT NULL DEFAULT 0
)";

const CREATE_INDEXES: &str = "
CREATE INDEX IF NOT EXISTS idx_created_at ON note (created_at);
CREATE INDEX IF NOT EXISTS idx_note_live ON note (deleted, created_at);";

/// One row per (note, embedding model). `note_updated_at` records which
/// version of the note text the vector was computed from.
const CREATE_EMBEDDING: &str = "CREATE TABLE IF NOT EXISTS embedding (
    id              INTEGER PRIMARY KEY,
    note_uuid       TEXT NOT NULL REFERENCES note(uuid4),
    model_id        TEXT NOT NULL,
    dim             INTEGER NOT NULL,
    vector          BLOB NOT NULL,
    note_updated_at TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    UNIQUE (note_uuid, model_id)
)";

/// Full-text index over text, user tags and AI tags. The trigram tokenizer
/// matches substrings, which also makes it work for CJK text that has no
/// spaces between words (terms need at least 3 characters; shorter terms fall
/// back to LIKE in [`crate::search`]).
const CREATE_FTS: &str = "
CREATE VIRTUAL TABLE IF NOT EXISTS note_fts USING fts5(
    txt, tags, ai_tags, content='note', content_rowid='rowid', tokenize='trigram'
);
CREATE TRIGGER IF NOT EXISTS note_fts_ai AFTER INSERT ON note BEGIN
    INSERT INTO note_fts(rowid, txt, tags, ai_tags) VALUES (new.rowid, new.txt, new.tags, new.ai_tags);
END;
CREATE TRIGGER IF NOT EXISTS note_fts_ad AFTER DELETE ON note BEGIN
    INSERT INTO note_fts(note_fts, rowid, txt, tags, ai_tags) VALUES ('delete', old.rowid, old.txt, old.tags, old.ai_tags);
END;
CREATE TRIGGER IF NOT EXISTS note_fts_au AFTER UPDATE OF txt, tags, ai_tags ON note BEGIN
    INSERT INTO note_fts(note_fts, rowid, txt, tags, ai_tags) VALUES ('delete', old.rowid, old.txt, old.tags, old.ai_tags);
    INSERT INTO note_fts(rowid, txt, tags, ai_tags) VALUES (new.rowid, new.txt, new.tags, new.ai_tags);
END;";

/// Register sqlite-vec for every connection opened afterwards.
pub(crate) fn register_sqlite_vec() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // SAFETY: `sqlite3_vec_init` has the signature SQLite expects for an
        // auto-extension entry point; this is the pattern the sqlite-vec crate
        // documents for rusqlite.
        unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Connection settings applied on every open.
pub(crate) fn configure(conn: &Connection, file_backed: bool) -> Result<()> {
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    if file_backed {
        // WAL lets the desktop app, MCP server and sync server share one file.
        let _mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
    }
    Ok(())
}

pub(crate) fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [name],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |r| r.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT meta_value FROM meta WHERE meta_key = ?1",
            [key],
            |r| r.get(0),
        )
        .optional()?)
}

pub(crate) fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (meta_key, meta_value) VALUES (?1, ?2)
         ON CONFLICT(meta_key) DO UPDATE SET meta_value = excluded.meta_value",
        [key, value],
    )?;
    Ok(())
}

/// Bring the database to [`SCHEMA_VERSION`]. Returns `true` when the vector
/// index must be rebuilt from the `embedding` table afterwards.
pub(crate) fn migrate(conn: &mut Connection) -> Result<bool> {
    let tx = conn.transaction()?;
    let has_note = table_exists(&tx, "note")?;
    let stored = if table_exists(&tx, "meta")? {
        get_meta(&tx, "version")?
    } else {
        None
    };
    let target = Version::parse(SCHEMA_VERSION).expect("SCHEMA_VERSION is valid semver");

    let rebuild = if has_note {
        let current = stored
            .as_deref()
            .and_then(|v| Version::parse(v).ok())
            .unwrap_or_else(|| Version::new(0, 0, 0));
        if current > target {
            return Err(Error::Migration(format!(
                "database schema {current} is newer than this build ({SCHEMA_VERSION}); update Fastxt"
            )));
        }
        if current < target {
            migrate_to_0_6(&tx)?;
            info!(from = %current, to = SCHEMA_VERSION, "database migrated");
            true
        } else {
            false
        }
    } else {
        create_fresh(&tx)?;
        info!(version = SCHEMA_VERSION, "database created");
        false
    };

    ensure_device_id(&tx)?;
    set_meta(&tx, "version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(rebuild)
}

fn create_fresh(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(CREATE_META)?;
    tx.execute_batch(CREATE_NOTE)?;
    tx.execute_batch(CREATE_INDEXES)?;
    tx.execute_batch(CREATE_EMBEDDING)?;
    tx.execute_batch(CREATE_FTS)?;
    Ok(())
}

fn ensure_device_id(tx: &Transaction<'_>) -> Result<()> {
    if get_meta(tx, "device_id")?.is_none() {
        set_meta(tx, "device_id", &uuid::Uuid::new_v4().simple().to_string())?;
    }
    Ok(())
}

/// Upgrade any pre-0.6 database (0.0.0 through 0.5.0).
fn migrate_to_0_6(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(CREATE_META)?;

    // The old FTS index could hold stale rows (INSERT OR REPLACE skipped its
    // delete trigger), so drop it before touching any note rows; it is rebuilt
    // from scratch at the end.
    tx.execute_batch(
        "DROP TRIGGER IF EXISTS note_ai;
         DROP TRIGGER IF EXISTS note_ad;
         DROP TRIGGER IF EXISTS note_au;
         DROP TABLE IF EXISTS note_fts;",
    )?;

    for (column, decl) in [
        ("ai_tags", "TEXT"),
        ("ai_summary", "TEXT"),
        ("ai_category", "TEXT"),
        ("updated_at", "TEXT NOT NULL DEFAULT ''"),
        ("ai_updated_at", "TEXT NOT NULL DEFAULT ''"),
        ("deleted", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !column_exists(tx, "note", column)? {
            tx.execute_batch(&format!("ALTER TABLE note ADD COLUMN {column} {decl}"))?;
        }
    }

    // Backfill version stamps from created_at (see clock::legacy_stamp).
    tx.execute(
        "UPDATE note SET updated_at = printf('%013x-0000-%s',
             COALESCE(CAST(strftime('%s', created_at) AS INTEGER), 0) * 1000, ?1)
         WHERE updated_at IS NULL OR updated_at = ''",
        [LEGACY_NODE],
    )?;
    // ai_tags was stored as JSONB by 0.4/0.5 and as text by sync; store text.
    tx.execute_batch(
        "UPDATE note SET ai_tags = CASE WHEN json_valid(ai_tags, 8) THEN json(ai_tags) ELSE NULL END
             WHERE typeof(ai_tags) = 'blob';
         UPDATE note SET ai_tags = NULL WHERE ai_tags = '';
         UPDATE note SET ai_summary = NULL WHERE ai_summary = '';
         UPDATE note SET ai_category = NULL WHERE ai_category = '';
         UPDATE note SET ai_updated_at = updated_at
             WHERE (ai_updated_at IS NULL OR ai_updated_at = '')
               AND (ai_tags IS NOT NULL OR ai_summary IS NOT NULL OR ai_category IS NOT NULL);",
    )?;

    tx.execute_batch(CREATE_INDEXES)?;
    tx.execute_batch(CREATE_EMBEDDING)?;

    // Move old embeddings (keyed by local rowid, labelled with the chat model
    // name or "unknown") into the new table. Every pre-0.6 embedding came from
    // Ollama, and "unknown" meant its default model, llama3.2.
    if table_exists(tx, "note_embedding")? {
        tx.execute_batch(
            "INSERT OR IGNORE INTO embedding (note_uuid, model_id, dim, vector, note_updated_at, created_at)
             SELECT n.uuid4,
                    'ollama:' || CASE WHEN e.model_id IN ('', 'unknown') THEN 'llama3.2' ELSE e.model_id END,
                    length(e.embedding) / 4, e.embedding, n.updated_at, e.created_at
             FROM note_embedding e JOIN note n ON n.rowid = e.note_rowid
             WHERE length(e.embedding) > 0 AND length(e.embedding) % 4 = 0;
             DROP TABLE note_embedding;",
        )?;
    }
    tx.execute_batch("DROP TABLE IF EXISTS vec_notes")?;

    tx.execute_batch(CREATE_FTS)?;
    tx.execute("INSERT INTO note_fts(note_fts) VALUES ('rebuild')", [])?;
    tx.execute("DELETE FROM meta WHERE meta_key = 'is_upgrading'", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::legacy_stamp;

    fn open() -> Connection {
        register_sqlite_vec();
        Connection::open_in_memory().unwrap()
    }

    /// The schema shipped by 0.5.0, with rows in every legacy shape.
    fn legacy_0_5(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE note (
                 rowid INTEGER PRIMARY KEY AUTOINCREMENT, uuid4 TEXT NOT NULL UNIQUE,
                 txt TEXT NOT NULL, tags TEXT NOT NULL, created_at TEXT NOT NULL,
                 ai_tags TEXT, ai_summary TEXT, ai_category TEXT);
             CREATE TABLE meta (meta_key TEXT PRIMARY KEY, meta_value TEXT NOT NULL);
             INSERT INTO meta VALUES ('version', '0.5.0');
             CREATE TABLE note_embedding (note_rowid INTEGER PRIMARY KEY REFERENCES note(rowid),
                 embedding BLOB NOT NULL, model_id TEXT NOT NULL, created_at TEXT NOT NULL);
             CREATE VIRTUAL TABLE note_fts USING fts5(txt, tags, content=note, content_rowid=rowid);
             INSERT INTO note (uuid4, txt, tags, created_at, ai_tags)
                 VALUES ('u1', 'first note', 'a,b', '2020-01-01 00:00:00', jsonb('[\"x\",\"y\"]'));
             INSERT INTO note (uuid4, txt, tags, created_at, ai_summary)
                 VALUES ('u2', '向量数据库笔记', '学习', '2021-06-01 12:00:00', 'a summary');",
        )
        .unwrap();
        let v: Vec<u8> = [1.0f32, 0.0, 0.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        conn.execute(
            "INSERT INTO note_embedding VALUES (1, ?1, 'unknown', '2021-01-01 00:00:00')",
            [v],
        )
        .unwrap();
    }

    #[test]
    fn fresh_database_gets_latest_schema() {
        let mut conn = open();
        assert!(!migrate(&mut conn).unwrap());
        assert_eq!(get_meta(&conn, "version").unwrap().unwrap(), SCHEMA_VERSION);
        assert!(get_meta(&conn, "device_id").unwrap().is_some());
        for t in ["note", "meta", "embedding", "note_fts"] {
            assert!(table_exists(&conn, t).unwrap(), "{t} missing");
        }
        // Idempotent.
        assert!(!migrate(&mut conn).unwrap());
    }

    #[test]
    fn upgrades_a_0_5_database() {
        let mut conn = open();
        legacy_0_5(&conn);
        assert!(migrate(&mut conn).unwrap(), "vector index needs a rebuild");

        let (stamp, ai_tags, ai_stamp): (String, String, String) = conn
            .query_row(
                "SELECT updated_at, ai_tags, ai_updated_at FROM note WHERE uuid4 = 'u1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        // SQL backfill agrees with the Rust definition.
        assert_eq!(stamp, legacy_stamp("2020-01-01 00:00:00"));
        assert_eq!(ai_tags, r#"["x","y"]"#, "JSONB converted to text");
        assert_eq!(ai_stamp, stamp);

        let (model, dim): (String, i64) = conn
            .query_row("SELECT model_id, dim FROM embedding", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((model.as_str(), dim), ("ollama:llama3.2", 3));
        assert!(!table_exists(&conn, "note_embedding").unwrap());

        // FTS rebuilt with the trigram tokenizer: CJK substring search works.
        let hits: i64 = conn
            .query_row(
                "SELECT count(*) FROM note_fts WHERE note_fts MATCH '\"数据库\"'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
        conn.execute(
            "INSERT INTO note_fts(note_fts, rank) VALUES ('integrity-check', 1)",
            [],
        )
        .unwrap();
        assert_eq!(get_meta(&conn, "version").unwrap().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn refuses_a_database_from_a_newer_build() {
        let mut conn = open();
        migrate(&mut conn).unwrap();
        set_meta(&conn, "version", "9.0.0").unwrap();
        assert!(matches!(migrate(&mut conn), Err(Error::Migration(_))));
    }
}
