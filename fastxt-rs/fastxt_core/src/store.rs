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

//! [`Fastxt`]: the typed handle every client uses to read and write notes.

use crate::clock::Clock;
use crate::error::{Error, Result};
use crate::model::{CategoryCount, NewNote, Note, NoteKey, Settings, Stamp, SyncNote, TagCount};
use crate::schema;
use crate::tags::{ai_tags_json, normalize_tags, parse_ai_tags, split_tags};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tracing::debug;

/// A handle shared between threads (desktop UI, MCP server, sync server).
pub type SharedDb = Arc<Mutex<Fastxt>>;

pub(crate) const NOTE_COLUMNS: &str = "n.rowid, n.uuid4, n.txt, n.tags, n.created_at, n.ai_tags, n.ai_summary, n.ai_category, n.updated_at, n.ai_updated_at";

pub(crate) fn note_from_row(row: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        rowid: row.get(0)?,
        uuid4: row.get(1)?,
        txt: row.get(2)?,
        tags: row.get(3)?,
        created_at: row.get(4)?,
        ai_tags: row.get(5)?,
        ai_summary: row.get(6)?,
        ai_category: row.get(7)?,
        updated_at: row.get(8)?,
        ai_updated_at: row.get(9)?,
    })
}

fn sync_note_from_row(row: &Row<'_>) -> rusqlite::Result<SyncNote> {
    Ok(SyncNote {
        uuid4: row.get(0)?,
        txt: row.get(1)?,
        tags: row.get(2)?,
        created_at: row.get(3)?,
        ai_tags: row.get(4)?,
        ai_summary: row.get(5)?,
        ai_category: row.get(6)?,
        updated_at: row.get(7)?,
        ai_updated_at: row.get(8)?,
        deleted: row.get::<_, i64>(9)? != 0,
    })
}

const SYNC_COLUMNS: &str = "uuid4, txt, tags, created_at, ai_tags, ai_summary, ai_category, updated_at, ai_updated_at, deleted";

/// Current UTC time in the stored `created_at` format.
pub(crate) fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn non_empty(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// Default database file for this platform, or `FASTXT_DB` when set.
///
/// Android has no usable default (apps can't write to `/sdcard` under scoped
/// storage); the app must pass its own directory via the FFI.
///
/// # Errors
/// Returns [`Error::Path`] when no location can be determined.
pub fn default_db_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("FASTXT_DB").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    if cfg!(target_os = "android") {
        return Err(Error::Path(
            "no default location on Android; the app must set a database directory".into(),
        ));
    }
    let home =
        dirs::home_dir().ok_or_else(|| Error::Path("cannot determine home directory".into()))?;
    let dir = if cfg!(target_os = "ios") {
        home.join("Documents")
    } else {
        home.join("Fastxt")
    };
    Ok(dir.join("fastxt.sqlite3"))
}

/// An open Fastxt database.
///
/// Owns one SQLite connection (WAL mode for files) and the device's hybrid
/// logical clock. Several handles may open the same file concurrently.
pub struct Fastxt {
    pub(crate) conn: Connection,
    path: Option<PathBuf>,
    device_id: String,
    pub(crate) clock: Clock,
}

impl std::fmt::Debug for Fastxt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fastxt")
            .field("path", &self.path)
            .field("device_id", &self.device_id)
            .finish_non_exhaustive()
    }
}

impl Fastxt {
    /// Open (creating and migrating if needed) the database at `path`.
    ///
    /// # Errors
    /// Fails if the directory can't be created, the file can't be opened, or a
    /// migration fails.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| {
                Error::Path(format!("cannot create directory {}: {e}", dir.display()))
            })?;
        }
        schema::register_sqlite_vec();
        let conn = Connection::open(path)?;
        schema::configure(&conn, true)?;
        Self::init(conn, Some(path.to_path_buf()))
    }

    /// Open the database at [`default_db_path`].
    ///
    /// # Errors
    /// See [`Fastxt::open`] and [`default_db_path`].
    pub fn open_default() -> Result<Self> {
        Self::open(default_db_path()?)
    }

    /// Open a private in-memory database (tests, previews).
    ///
    /// # Errors
    /// Fails only if SQLite can't allocate the database.
    pub fn open_in_memory() -> Result<Self> {
        schema::register_sqlite_vec();
        let conn = Connection::open_in_memory()?;
        schema::configure(&conn, false)?;
        Self::init(conn, None)
    }

    fn init(mut conn: Connection, path: Option<PathBuf>) -> Result<Self> {
        let rebuild = schema::migrate(&mut conn)?;
        let device_id = schema::get_meta(&conn, "device_id")?
            .ok_or_else(|| Error::Migration("device id missing after migration".into()))?;
        let mut clock = Clock::new(device_id.clone());
        let (max_user, max_ai): (Option<String>, Option<String>) = conn.query_row(
            "SELECT max(updated_at), max(ai_updated_at) FROM note",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        for stamp in [max_user, max_ai].into_iter().flatten() {
            clock.observe(&stamp);
        }
        let mut db = Fastxt {
            conn,
            path,
            device_id,
            clock,
        };
        if rebuild {
            db.rebuild_vector_index()?;
        }
        Ok(db)
    }

    /// The database file, or `None` for an in-memory database.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// This database's device id (also the node id in its clock stamps).
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// The stored schema version.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn schema_version(&self) -> Result<String> {
        Ok(schema::get_meta(&self.conn, "version")?.unwrap_or_default())
    }

    // ----------------------------------------------------------------------
    // Notes
    // ----------------------------------------------------------------------

    fn rowid_of(&self, key: &NoteKey, include_deleted: bool) -> Result<Option<i64>> {
        let live = if include_deleted {
            ""
        } else {
            " AND deleted = 0"
        };
        let found = match key {
            NoteKey::Rowid(r) => self
                .conn
                .query_row(
                    &format!("SELECT rowid FROM note WHERE rowid = ?1{live}"),
                    [r],
                    |row| row.get(0),
                )
                .optional()?,
            NoteKey::Uuid(u) => self
                .conn
                .query_row(
                    &format!("SELECT rowid FROM note WHERE uuid4 = ?1{live}"),
                    [u],
                    |row| row.get(0),
                )
                .optional()?,
        };
        Ok(found)
    }

    fn require(&self, key: &NoteKey) -> Result<Note> {
        self.get(key)?
            .ok_or_else(|| Error::NotFound(key.to_string()))
    }

    /// Fetch a note that hasn't been deleted.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn get(&self, key: &NoteKey) -> Result<Option<Note>> {
        let Some(rowid) = self.rowid_of(key, false)? else {
            return Ok(None);
        };
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {NOTE_COLUMNS} FROM note n WHERE n.rowid = ?1"),
                [rowid],
                note_from_row,
            )
            .optional()?)
    }

    /// Save a new note and return it (with its rowid and UUID).
    ///
    /// # Errors
    /// [`Error::Invalid`] if the text is blank; database errors otherwise.
    pub fn insert(&mut self, new: NewNote) -> Result<Note> {
        if new.txt.trim().is_empty() {
            return Err(Error::Invalid("note text is empty".into()));
        }
        let stamp = self.clock.tick();
        let ai_tags = new
            .ai_tags
            .as_deref()
            .filter(|t| !t.is_empty())
            .map(ai_tags_json);
        let ai_summary = non_empty(new.ai_summary.as_deref()).map(str::to_string);
        let ai_category = non_empty(new.ai_category.as_deref()).map(str::to_string);
        let has_ai = ai_tags.is_some() || ai_summary.is_some() || ai_category.is_some();
        self.conn.execute(
            "INSERT INTO note (uuid4, txt, tags, created_at, ai_tags, ai_summary, ai_category,
                               updated_at, ai_updated_at, deleted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![
                uuid::Uuid::new_v4().to_string(),
                new.txt,
                normalize_tags(&new.tags),
                now_utc(),
                ai_tags,
                ai_summary,
                ai_category,
                stamp,
                if has_ai { stamp.as_str() } else { "" },
            ],
        )?;
        let rowid = self.conn.last_insert_rowid();
        self.require(&NoteKey::Rowid(rowid))
    }

    /// Change a note's text and tags. Unchanged input leaves the note (and its
    /// version stamp) untouched. Changing the text drops its embeddings, which
    /// no longer describe it.
    ///
    /// # Errors
    /// [`Error::NotFound`] for a missing or deleted note; [`Error::Invalid`]
    /// for blank text.
    pub fn update(&mut self, key: &NoteKey, txt: &str, tags: &str) -> Result<Note> {
        if txt.trim().is_empty() {
            return Err(Error::Invalid("note text is empty".into()));
        }
        let note = self.require(key)?;
        let tags = normalize_tags(tags);
        if note.txt == txt && note.tags == tags {
            return Ok(note);
        }
        let stamp = self.clock.tick_after(&note.updated_at);
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE note SET txt = ?1, tags = ?2, updated_at = ?3 WHERE rowid = ?4",
            params![txt, tags, stamp, note.rowid],
        )?;
        if note.txt != txt {
            crate::vector::delete_embeddings(&tx, &note.uuid4)?;
        }
        tx.commit()?;
        self.require(&NoteKey::Rowid(note.rowid))
    }

    /// Delete a note. The row stays as a tombstone (content blanked) so the
    /// deletion reaches other devices on the next sync instead of the note
    /// coming back. Returns `false` if there was nothing to delete.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn delete(&mut self, key: &NoteKey) -> Result<bool> {
        let Some(note) = self.get(key)? else {
            return Ok(false);
        };
        let prev = note.updated_at.as_str().max(note.ai_updated_at.as_str());
        let stamp = self.clock.tick_after(prev);
        let tx = self.conn.transaction()?;
        crate::vector::delete_embeddings(&tx, &note.uuid4)?;
        tx.execute(
            "UPDATE note SET txt = '', tags = '', ai_tags = NULL, ai_summary = NULL,
                 ai_category = NULL, deleted = 1, updated_at = ?1, ai_updated_at = ?1
             WHERE rowid = ?2",
            params![stamp, note.rowid],
        )?;
        tx.commit()?;
        debug!(uuid = %note.uuid4, "note deleted (tombstone)");
        Ok(true)
    }

    /// Number of notes (excluding deleted ones).
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn count(&self) -> Result<u32> {
        Ok(self
            .conn
            .query_row("SELECT count(*) FROM note WHERE deleted = 0", [], |r| {
                r.get(0)
            })?)
    }

    // ----------------------------------------------------------------------
    // AI metadata
    // ----------------------------------------------------------------------

    fn bump_ai(&mut self, note: &Note, sql: &str, value: Option<String>) -> Result<()> {
        let stamp = self.clock.tick_after(&note.ai_updated_at);
        self.conn.execute(sql, params![value, stamp, note.rowid])?;
        Ok(())
    }

    /// Store AI tags for a note (an empty list clears them).
    ///
    /// # Errors
    /// [`Error::NotFound`] for a missing note.
    pub fn set_ai_tags(&mut self, key: &NoteKey, tags: &[String]) -> Result<()> {
        let note = self.require(key)?;
        let value = (!tags.is_empty()).then(|| ai_tags_json(tags));
        self.bump_ai(
            &note,
            "UPDATE note SET ai_tags = ?1, ai_updated_at = ?2 WHERE rowid = ?3",
            value,
        )
    }

    /// Store an AI summary for a note (blank clears it).
    ///
    /// # Errors
    /// [`Error::NotFound`] for a missing note.
    pub fn set_ai_summary(&mut self, key: &NoteKey, summary: &str) -> Result<()> {
        let note = self.require(key)?;
        let value = non_empty(Some(summary)).map(str::to_string);
        self.bump_ai(
            &note,
            "UPDATE note SET ai_summary = ?1, ai_updated_at = ?2 WHERE rowid = ?3",
            value,
        )
    }

    /// Store (or clear) a note's AI category.
    ///
    /// # Errors
    /// [`Error::NotFound`] for a missing note.
    pub fn set_ai_category(&mut self, key: &NoteKey, category: Option<&str>) -> Result<()> {
        let note = self.require(key)?;
        let value = non_empty(category).map(str::to_string);
        self.bump_ai(
            &note,
            "UPDATE note SET ai_category = ?1, ai_updated_at = ?2 WHERE rowid = ?3",
            value,
        )
    }

    fn notes_where(&self, condition: &str, limit: u32) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM note n WHERE n.deleted = 0 AND ({condition})
             ORDER BY n.created_at DESC, n.rowid DESC LIMIT ?1"
        ))?;
        let notes = stmt
            .query_map([limit], note_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(notes)
    }

    fn count_where(&self, condition: &str) -> Result<u32> {
        Ok(self.conn.query_row(
            &format!("SELECT count(*) FROM note n WHERE n.deleted = 0 AND ({condition})"),
            [],
            |r| r.get(0),
        )?)
    }

    /// Notes that have no AI tags yet, newest first.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn notes_without_ai_tags(&self, limit: u32) -> Result<Vec<Note>> {
        self.notes_where("n.ai_tags IS NULL", limit)
    }

    /// How many notes have no AI tags yet.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn count_without_ai_tags(&self) -> Result<u32> {
        self.count_where("n.ai_tags IS NULL")
    }

    /// Notes that have no AI category yet, newest first.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn notes_without_category(&self, limit: u32) -> Result<Vec<Note>> {
        self.notes_where("n.ai_category IS NULL", limit)
    }

    /// How many notes have no AI category yet.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn count_without_category(&self) -> Result<u32> {
        self.count_where("n.ai_category IS NULL")
    }

    /// AI categories with note counts, largest first.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn categories(&self) -> Result<Vec<CategoryCount>> {
        let mut stmt = self.conn.prepare(
            "SELECT ai_category, count(*) FROM note
             WHERE deleted = 0 AND ai_category IS NOT NULL
             GROUP BY ai_category ORDER BY count(*) DESC, ai_category",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CategoryCount {
                    category: r.get(0)?,
                    count: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    fn recategorize(&mut self, from: &str, to: Option<&str>) -> Result<usize> {
        let stamp = self.clock.tick();
        Ok(self.conn.execute(
            "UPDATE note SET ai_category = ?1, ai_updated_at = ?2
             WHERE deleted = 0 AND ai_category = ?3",
            params![to, stamp, from],
        )?)
    }

    /// Rename a category on every note that has it. Later "organize" runs
    /// reuse existing categories, so the new name sticks.
    ///
    /// # Errors
    /// [`Error::Invalid`] for a blank new name.
    pub fn rename_category(&mut self, old: &str, new: &str) -> Result<usize> {
        let new = non_empty(Some(new))
            .ok_or_else(|| Error::Invalid("new category name is empty".into()))?
            .to_string();
        self.recategorize(old, Some(&new))
    }

    /// Clear a category from every note that has it.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn dismiss_category(&mut self, category: &str) -> Result<usize> {
        self.recategorize(category, None)
    }

    /// The most-used tags (user and AI combined), most frequent first.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn tag_vocabulary(&self, limit: usize) -> Result<Vec<TagCount>> {
        let mut stmt = self
            .conn
            .prepare("SELECT tags, ai_tags FROM note WHERE deleted = 0")?;
        let mut counts: HashMap<String, (String, u32)> = HashMap::new();
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (tags, ai_tags) = row?;
            let mut all = split_tags(&tags);
            all.extend(ai_tags.as_deref().map(parse_ai_tags).unwrap_or_default());
            let mut seen_in_note = std::collections::HashSet::new();
            for tag in all {
                let key = tag.to_lowercase();
                if seen_in_note.insert(key.clone()) {
                    counts.entry(key).or_insert((tag, 0)).1 += 1;
                }
            }
        }
        let mut out: Vec<TagCount> = counts
            .into_values()
            .map(|(tag, count)| TagCount { tag, count })
            .collect();
        out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.tag.cmp(&b.tag)));
        out.truncate(limit);
        Ok(out)
    }

    // ----------------------------------------------------------------------
    // Settings
    // ----------------------------------------------------------------------

    /// Stored settings, or defaults when none have been saved.
    ///
    /// # Errors
    /// Fails on a database error; unreadable stored JSON falls back to defaults.
    pub fn settings(&self) -> Result<Settings> {
        Ok(schema::get_meta(&self.conn, "settings")?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default())
    }

    /// Persist settings.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn save_settings(&mut self, settings: &Settings) -> Result<()> {
        schema::set_meta(&self.conn, "settings", &serde_json::to_string(settings)?)
    }

    // ----------------------------------------------------------------------
    // Sync support
    // ----------------------------------------------------------------------

    /// Version stamps of every note, tombstones included.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn manifest(&self) -> Result<Vec<Stamp>> {
        let mut stmt = self
            .conn
            .prepare("SELECT uuid4, updated_at, ai_updated_at FROM note")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Stamp {
                    uuid4: r.get(0)?,
                    updated_at: r.get(1)?,
                    ai_updated_at: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Full records (tombstones included) for the given UUIDs.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn records(&self, uuids: &[String]) -> Result<Vec<SyncNote>> {
        let mut out = Vec::with_capacity(uuids.len());
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {SYNC_COLUMNS} FROM note WHERE uuid4 = ?1"))?;
        for uuid in uuids {
            if let Some(rec) = stmt.query_row([uuid], sync_note_from_row).optional()? {
                out.push(rec);
            }
        }
        Ok(out)
    }

    /// Merge notes received from a peer. For each note the user fields (text,
    /// tags, deletion) and the AI fields are merged independently: the side
    /// with the newer clock stamp wins each group. Returns how many notes
    /// changed locally.
    ///
    /// # Errors
    /// Fails on a database error; the whole batch is rolled back.
    pub fn apply_remote(&mut self, records: &[SyncNote]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for rec in records {
            if rec.uuid4.is_empty() {
                continue;
            }
            self.clock.observe(&rec.updated_at);
            self.clock.observe(&rec.ai_updated_at);
            let local = tx
                .query_row(
                    &format!("SELECT {SYNC_COLUMNS} FROM note WHERE uuid4 = ?1"),
                    [&rec.uuid4],
                    sync_note_from_row,
                )
                .optional()?;
            let Some(local) = local else {
                let (txt, tags, ai_tags, ai_summary, ai_category) = if rec.deleted {
                    (String::new(), String::new(), None, None, None)
                } else {
                    (
                        rec.txt.clone(),
                        rec.tags.clone(),
                        rec.ai_tags.clone(),
                        rec.ai_summary.clone(),
                        rec.ai_category.clone(),
                    )
                };
                tx.execute(
                    "INSERT INTO note (uuid4, txt, tags, created_at, ai_tags, ai_summary,
                                       ai_category, updated_at, ai_updated_at, deleted)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        rec.uuid4,
                        txt,
                        tags,
                        rec.created_at,
                        ai_tags,
                        ai_summary,
                        ai_category,
                        rec.updated_at,
                        rec.ai_updated_at,
                        rec.deleted
                    ],
                )?;
                changed += 1;
                continue;
            };

            let user_newer = rec.updated_at > local.updated_at;
            let ai_newer = rec.ai_updated_at > local.ai_updated_at;
            if !user_newer && !ai_newer {
                continue;
            }
            let mut merged = local.clone();
            if user_newer {
                merged.txt.clone_from(&rec.txt);
                merged.tags.clone_from(&rec.tags);
                merged.deleted = rec.deleted;
                merged.updated_at.clone_from(&rec.updated_at);
                if merged.created_at.is_empty() {
                    merged.created_at.clone_from(&rec.created_at);
                }
            }
            if ai_newer {
                merged.ai_tags.clone_from(&rec.ai_tags);
                merged.ai_summary.clone_from(&rec.ai_summary);
                merged.ai_category.clone_from(&rec.ai_category);
                merged.ai_updated_at.clone_from(&rec.ai_updated_at);
            }
            // A deleted note keeps no content, whichever side's AI fields won.
            if merged.deleted {
                merged.txt.clear();
                merged.tags.clear();
                merged.ai_tags = None;
                merged.ai_summary = None;
                merged.ai_category = None;
            }
            if merged.deleted || merged.txt != local.txt {
                crate::vector::delete_embeddings(&tx, &merged.uuid4)?;
            }
            tx.execute(
                "UPDATE note SET txt = ?1, tags = ?2, created_at = ?3, ai_tags = ?4,
                     ai_summary = ?5, ai_category = ?6, updated_at = ?7, ai_updated_at = ?8,
                     deleted = ?9
                 WHERE uuid4 = ?10",
                params![
                    merged.txt,
                    merged.tags,
                    merged.created_at,
                    merged.ai_tags,
                    merged.ai_summary,
                    merged.ai_category,
                    merged.updated_at,
                    merged.ai_updated_at,
                    merged.deleted,
                    merged.uuid4
                ],
            )?;
            changed += 1;
        }
        tx.commit()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Fastxt {
        Fastxt::open_in_memory().unwrap()
    }

    fn add(db: &mut Fastxt, txt: &str, tags: &str) -> Note {
        db.insert(NewNote::new(txt, tags)).unwrap()
    }

    fn fts_ok(db: &Fastxt) {
        db.conn
            .execute(
                "INSERT INTO note_fts(note_fts, rank) VALUES ('integrity-check', 1)",
                [],
            )
            .expect("FTS index consistent with note table");
    }

    #[test]
    fn insert_returns_the_created_note() {
        let mut db = db();
        let n = add(&mut db, "hello", "a, b ,a");
        assert!(n.rowid > 0);
        assert_eq!(n.uuid4.len(), 36);
        assert_eq!(n.tags, "a,b");
        assert!(!n.updated_at.is_empty());
        assert_eq!(n.ai_updated_at, "");
        assert_eq!(db.get(&NoteKey::Uuid(n.uuid4.clone())).unwrap(), Some(n));
    }

    #[test]
    fn insert_rejects_blank_text() {
        assert!(matches!(
            db().insert(NewNote::new("  ", "")),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn insert_can_carry_ai_fields() {
        let mut db = db();
        let n = db
            .insert(NewNote {
                txt: "t".into(),
                ai_summary: Some("sum".into()),
                ai_tags: Some(vec!["x".into()]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(n.ai_summary.as_deref(), Some("sum"));
        assert_eq!(n.ai_tag_list(), vec!["x"]);
        assert_eq!(n.ai_updated_at, n.updated_at);
    }

    #[test]
    fn update_keeps_rowid_bumps_stamp_and_keeps_fts_consistent() {
        let mut db = db();
        let n = add(&mut db, "original text", "t");
        let u = db
            .update(&NoteKey::Rowid(n.rowid), "edited text", "t, u")
            .unwrap();
        assert_eq!(u.rowid, n.rowid);
        assert_eq!(u.uuid4, n.uuid4);
        assert_eq!(u.tags, "t,u");
        assert!(u.updated_at > n.updated_at);
        fts_ok(&db);
        // Unchanged input is a no-op.
        let again = db
            .update(&NoteKey::Rowid(n.rowid), "edited text", "t,u")
            .unwrap();
        assert_eq!(again.updated_at, u.updated_at);
    }

    #[test]
    fn delete_leaves_a_blank_tombstone() {
        let mut db = db();
        let n = add(&mut db, "secret", "private");
        db.set_ai_summary(&NoteKey::Rowid(n.rowid), "a secret summary")
            .unwrap();
        assert!(db.delete(&NoteKey::Rowid(n.rowid)).unwrap());
        assert!(!db.delete(&NoteKey::Rowid(n.rowid)).unwrap());
        assert_eq!(db.get(&NoteKey::Rowid(n.rowid)).unwrap(), None);
        assert_eq!(db.count().unwrap(), 0);
        let rec = &db.records(std::slice::from_ref(&n.uuid4)).unwrap()[0];
        assert!(rec.deleted);
        assert_eq!((rec.txt.as_str(), rec.tags.as_str()), ("", ""));
        assert_eq!(rec.ai_summary, None);
        fts_ok(&db);
    }

    #[test]
    fn ai_setters_bump_only_the_ai_stamp() {
        let mut db = db();
        let n = add(&mut db, "t", "");
        let key = NoteKey::Rowid(n.rowid);
        db.set_ai_tags(&key, &["Rust".into(), "rust".into(), "sync".into()])
            .unwrap();
        db.set_ai_category(&key, Some("work")).unwrap();
        let after = db.get(&key).unwrap().unwrap();
        assert_eq!(after.ai_tag_list(), vec!["Rust", "sync"]);
        assert_eq!(after.ai_category.as_deref(), Some("work"));
        assert_eq!(after.updated_at, n.updated_at);
        assert!(after.ai_updated_at > n.updated_at);
    }

    #[test]
    fn categories_rename_and_dismiss() {
        let mut db = db();
        for (t, c) in [("a", "work"), ("b", "work"), ("c", "home")] {
            let n = add(&mut db, t, "");
            db.set_ai_category(&NoteKey::Rowid(n.rowid), Some(c))
                .unwrap();
        }
        assert_eq!(
            db.categories().unwrap(),
            vec![
                CategoryCount {
                    category: "work".into(),
                    count: 2
                },
                CategoryCount {
                    category: "home".into(),
                    count: 1
                },
            ]
        );
        assert_eq!(db.rename_category("work", "job").unwrap(), 2);
        assert_eq!(db.dismiss_category("home").unwrap(), 1);
        assert_eq!(db.categories().unwrap().len(), 1);
        assert_eq!(db.count_without_category().unwrap(), 1);
    }

    #[test]
    fn tag_vocabulary_counts_user_and_ai_tags_once_per_note() {
        let mut db = db();
        let n = add(&mut db, "a", "rust, sync");
        db.set_ai_tags(&NoteKey::Rowid(n.rowid), &["Rust".into(), "db".into()])
            .unwrap();
        add(&mut db, "b", "rust");
        let vocab = db.tag_vocabulary(10).unwrap();
        assert_eq!(
            vocab[0],
            TagCount {
                tag: "rust".into(),
                count: 2
            }
        );
        assert_eq!(vocab.len(), 3);
    }

    #[test]
    fn settings_round_trip() {
        let mut db = db();
        let mut s = db.settings().unwrap();
        assert_eq!(s.ai.backend, "ollama");
        s.ai.embedding_model = "bge-m3".into();
        db.save_settings(&s).unwrap();
        assert_eq!(db.settings().unwrap(), s);
    }

    #[test]
    fn file_database_reopens_with_monotonic_clock() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/fastxt.sqlite3");
        let first = {
            let mut db = Fastxt::open(&path).unwrap();
            add(&mut db, "persisted", "")
        };
        let mut db = Fastxt::open(&path).unwrap();
        assert_eq!(db.count().unwrap(), 1);
        let second = add(&mut db, "later", "");
        assert!(second.updated_at > first.updated_at);
    }

    // ---- merge ---------------------------------------------------------

    fn pair() -> (Fastxt, Fastxt) {
        (db(), db())
    }

    fn sync_all(from: &Fastxt, to: &mut Fastxt) -> usize {
        let uuids: Vec<String> = from
            .manifest()
            .unwrap()
            .into_iter()
            .map(|s| s.uuid4)
            .collect();
        to.apply_remote(&from.records(&uuids).unwrap()).unwrap()
    }

    #[test]
    fn remote_insert_edit_and_delete_propagate() {
        let (mut a, mut b) = pair();
        let n = add(&mut a, "draft", "x");
        assert_eq!(sync_all(&a, &mut b), 1);
        let key = NoteKey::Uuid(n.uuid4.clone());
        assert_eq!(b.get(&key).unwrap().unwrap().txt, "draft");

        a.update(&key, "final", "x").unwrap();
        sync_all(&a, &mut b);
        assert_eq!(b.get(&key).unwrap().unwrap().txt, "final");

        a.delete(&key).unwrap();
        sync_all(&a, &mut b);
        assert_eq!(b.get(&key).unwrap(), None, "deletion propagated");
        // Syncing back does not resurrect it on A.
        sync_all(&b, &mut a);
        assert_eq!(a.get(&key).unwrap(), None);
        fts_ok(&a);
        fts_ok(&b);
    }

    #[test]
    fn concurrent_text_edit_and_ai_update_both_survive() {
        let (mut a, mut b) = pair();
        let n = add(&mut a, "v1", "");
        sync_all(&a, &mut b);
        let key = NoteKey::Uuid(n.uuid4.clone());
        a.update(&key, "v2 from A", "").unwrap();
        b.set_ai_summary(&key, "summary from B").unwrap();
        sync_all(&a, &mut b);
        sync_all(&b, &mut a);
        for db in [&a, &b] {
            let note = db.get(&key).unwrap().unwrap();
            assert_eq!(note.txt, "v2 from A");
            assert_eq!(note.ai_summary.as_deref(), Some("summary from B"));
        }
        assert_eq!(a.manifest().unwrap(), b.manifest().unwrap(), "converged");
    }

    #[test]
    fn delete_wins_over_older_edit_and_strips_later_ai_data() {
        let (mut a, mut b) = pair();
        let n = add(&mut a, "v1", "");
        sync_all(&a, &mut b);
        let key = NoteKey::Uuid(n.uuid4.clone());
        b.update(&key, "edit on B", "").unwrap();
        a.delete(&key).unwrap(); // later stamp than B's edit
        b.set_ai_summary(&key, "late summary").unwrap(); // even later AI stamp
        sync_all(&a, &mut b);
        sync_all(&b, &mut a);
        for db in [&a, &b] {
            assert_eq!(db.get(&key).unwrap(), None);
            let rec = &db.records(std::slice::from_ref(&n.uuid4)).unwrap()[0];
            assert!(rec.deleted);
            assert_eq!(rec.ai_summary, None, "no content survives a delete");
        }
        assert_eq!(a.manifest().unwrap(), b.manifest().unwrap());
    }

    #[test]
    fn applying_the_same_records_twice_changes_nothing() {
        let (mut a, mut b) = pair();
        add(&mut a, "one", "");
        add(&mut a, "two", "");
        assert_eq!(sync_all(&a, &mut b), 2);
        assert_eq!(sync_all(&a, &mut b), 0);
    }

    #[test]
    fn local_edit_after_sync_beats_a_peer_with_a_fast_clock() {
        let (mut a, mut b) = pair();
        let n = add(&mut a, "v1", "");
        let key = NoteKey::Uuid(n.uuid4.clone());
        // A peer whose clock runs an hour ahead edits the note.
        let mut rec = a.records(std::slice::from_ref(&n.uuid4)).unwrap().remove(0);
        let (millis, _) = crate::clock::parse(&rec.updated_at).unwrap();
        rec.txt = "from the future".into();
        rec.updated_at = format!("{:013x}-0000-ffff", millis + 3_600_000);
        b.apply_remote(std::slice::from_ref(&rec)).unwrap();
        // B edits afterwards: its clock observed the future stamp, so it wins.
        b.update(&key, "edited after sync", "").unwrap();
        assert!(b.get(&key).unwrap().unwrap().updated_at > rec.updated_at);
    }
}
