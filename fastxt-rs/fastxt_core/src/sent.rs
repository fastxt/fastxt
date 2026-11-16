/*
    Fastxt
    Copyright (C) 2026  Yi Wang

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

//! What was sent where: one record per time a note was typed into a
//! coding-agent session.
//!
//! A record is written once, by the device whose computer did the typing,
//! and never changed, so devices merge them by id: each takes the records it
//! lacks. A record goes when its note is deleted, and the note's tombstone
//! keeps it from coming back.

use crate::error::{Error, Result};
use crate::model::Sent;
use crate::store::{Fastxt, now_utc};
use rusqlite::{Connection, OptionalExtension, Row, params};

const SENT_COLUMNS: &str = "uuid4, note_uuid, sent_at, host, agent, session, cwd, started";

fn sent_from_row(row: &Row<'_>) -> rusqlite::Result<Sent> {
    Ok(Sent {
        uuid4: row.get(0)?,
        note_uuid: row.get(1)?,
        sent_at: row.get(2)?,
        host: row.get(3)?,
        agent: row.get(4)?,
        session: row.get(5)?,
        cwd: row.get(6)?,
        started: row.get::<_, i64>(7)? != 0,
    })
}

/// What the writer of a [`Sent`] record supplies; its id and time are set
/// when it is stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewSent {
    pub note_uuid: String,
    pub host: String,
    pub agent: String,
    pub session: String,
    pub cwd: String,
    pub started: bool,
}

impl NewSent {
    /// The record, with a new id and the time now.
    #[must_use]
    pub fn into_record(self) -> Sent {
        Sent {
            uuid4: uuid::Uuid::new_v4().to_string(),
            note_uuid: self.note_uuid,
            sent_at: now_utc(),
            host: self.host,
            agent: self.agent,
            session: self.session,
            cwd: self.cwd,
            started: self.started,
        }
    }
}

/// Drop the records of a note that is being deleted.
pub(crate) fn delete_for_note(conn: &Connection, note_uuid: &str) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM sent WHERE note_uuid = ?1", [note_uuid])
}

fn insert(conn: &Connection, sent: &Sent) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT OR IGNORE INTO sent (uuid4, note_uuid, sent_at, host, agent, session, cwd, started)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            sent.uuid4,
            sent.note_uuid,
            sent.sent_at,
            sent.host,
            sent.agent,
            sent.session,
            sent.cwd,
            sent.started
        ],
    )
}

impl Fastxt {
    /// Record that a note was typed into a session just now.
    ///
    /// # Errors
    /// [`Error::Invalid`] without a note; database errors otherwise.
    pub fn record_sent(&mut self, new: NewSent) -> Result<Sent> {
        if new.note_uuid.trim().is_empty() {
            return Err(Error::Invalid(
                "a sent record needs the note it is about".into(),
            ));
        }
        let sent = new.into_record();
        insert(&self.conn, &sent)?;
        Ok(sent)
    }

    /// What was sent, newest first: one note's records, or everyone's.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn sent(&self, note_uuid: Option<&str>, limit: u32) -> Result<Vec<Sent>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {SENT_COLUMNS} FROM sent WHERE ?1 IS NULL OR note_uuid = ?1
             ORDER BY sent_at DESC, rowid DESC LIMIT ?2"
        ))?;
        let rows = stmt
            .query_map(params![note_uuid, limit], sent_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The id of every sent record, for sync.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn sent_manifest(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT uuid4 FROM sent")?;
        let rows = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// The sent records with these ids, for sync.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn sent_records(&self, uuids: &[String]) -> Result<Vec<Sent>> {
        let mut out = Vec::with_capacity(uuids.len());
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {SENT_COLUMNS} FROM sent WHERE uuid4 = ?1"))?;
        for uuid in uuids {
            if let Some(sent) = stmt.query_row([uuid], sent_from_row).optional()? {
                out.push(sent);
            }
        }
        Ok(out)
    }

    /// Store sent records received from a peer. A record this device has is
    /// left as it is, and one about a note deleted here is dropped. Returns
    /// how many were new.
    ///
    /// # Errors
    /// Fails on a database error; the whole batch is rolled back.
    pub fn apply_remote_sent(&mut self, records: &[Sent]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut stored = 0;
        for sent in records {
            if sent.uuid4.is_empty() || sent.note_uuid.is_empty() {
                continue;
            }
            let deleted: Option<bool> = tx
                .query_row(
                    "SELECT deleted FROM note WHERE uuid4 = ?1",
                    [&sent.note_uuid],
                    |r| Ok(r.get::<_, i64>(0)? != 0),
                )
                .optional()?;
            // A note not here yet may still arrive; one deleted here won't.
            if deleted == Some(true) {
                continue;
            }
            stored += insert(&tx, sent)?;
        }
        tx.commit()?;
        Ok(stored)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NewNote, NoteKey, SyncNote};

    fn db() -> Fastxt {
        Fastxt::open_in_memory().unwrap()
    }

    fn new_sent(note_uuid: &str, session: &str) -> NewSent {
        NewSent {
            note_uuid: note_uuid.to_string(),
            host: "orca".into(),
            agent: "claude".into(),
            session: session.to_string(),
            cwd: "/code/api".into(),
            started: true,
        }
    }

    #[test]
    fn a_sent_record_keeps_where_and_when_and_lists_newest_first() {
        let mut db = db();
        let note = db.insert(NewNote::new("Fix the login test", "")).unwrap();
        let other = db.insert(NewNote::new("Another", "")).unwrap();
        let first = db.record_sent(new_sent(&note.uuid4, "Fix login")).unwrap();
        let second = db
            .record_sent(new_sent(&note.uuid4, "Fix login 2"))
            .unwrap();
        db.record_sent(new_sent(&other.uuid4, "Elsewhere")).unwrap();

        assert_eq!(first.note_uuid, note.uuid4);
        assert_eq!(first.session, "Fix login");
        assert!(first.started);
        assert_eq!(first.sent_at.len(), "2026-01-01 00:00:00".len());
        assert_ne!(first.uuid4, second.uuid4);

        let mine = db.sent(Some(&note.uuid4), 10).unwrap();
        assert_eq!(
            mine.iter().map(|s| s.session.as_str()).collect::<Vec<_>>(),
            ["Fix login 2", "Fix login"],
            "newest first, even within one second"
        );
        assert_eq!(db.sent(None, 10).unwrap().len(), 3);
        assert_eq!(db.sent(None, 2).unwrap().len(), 2);
        assert!(db.sent(Some("no-such-note"), 10).unwrap().is_empty());
    }

    #[test]
    fn a_record_needs_its_note() {
        let mut db = db();
        assert!(matches!(
            db.record_sent(new_sent("  ", "s")),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn devices_merge_sent_records_by_id() {
        let (mut a, mut b) = (db(), db());
        let note = a.insert(NewNote::new("Fix the login test", "")).unwrap();
        let sent = a.record_sent(new_sent(&note.uuid4, "Fix login")).unwrap();

        let ids = a.sent_manifest().unwrap();
        assert_eq!(ids, std::slice::from_ref(&sent.uuid4));
        let records = a.sent_records(&ids).unwrap();
        // The note isn't on B yet: the record is kept for when it arrives.
        assert_eq!(b.apply_remote_sent(&records).unwrap(), 1);
        assert_eq!(b.apply_remote_sent(&records).unwrap(), 0, "already there");
        assert_eq!(b.sent(None, 10).unwrap(), [sent]);
        assert!(
            a.sent_records(&["no-such-id".to_string()])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn deleting_a_note_takes_its_sent_records_and_they_do_not_come_back() {
        let (mut a, mut b) = (db(), db());
        let note = a.insert(NewNote::new("Fix the login test", "")).unwrap();
        let key = NoteKey::Uuid(note.uuid4.clone());
        let sent = a.record_sent(new_sent(&note.uuid4, "Fix login")).unwrap();
        b.apply_remote(&a.records(std::slice::from_ref(&note.uuid4)).unwrap())
            .unwrap();
        b.apply_remote_sent(std::slice::from_ref(&sent)).unwrap();

        assert!(a.delete(&key).unwrap());
        assert!(a.sent(None, 10).unwrap().is_empty(), "gone with its note");

        // B still has the record and offers it back: A's tombstone refuses.
        assert_eq!(a.apply_remote_sent(std::slice::from_ref(&sent)).unwrap(), 0);
        assert!(a.sent(None, 10).unwrap().is_empty());

        // The tombstone reaches B and takes B's copy with it.
        b.apply_remote(&a.records(std::slice::from_ref(&note.uuid4)).unwrap())
            .unwrap();
        assert!(b.sent(None, 10).unwrap().is_empty());
    }

    #[test]
    fn a_tombstone_for_a_note_never_seen_still_clears_its_records() {
        let mut db = db();
        let sent = Sent {
            uuid4: "s1".into(),
            note_uuid: "n1".into(),
            sent_at: "2026-01-01 00:00:00".into(),
            host: "orca".into(),
            agent: "claude".into(),
            session: "Fix login".into(),
            cwd: "/code/api".into(),
            started: false,
        };
        assert_eq!(
            db.apply_remote_sent(std::slice::from_ref(&sent)).unwrap(),
            1
        );
        db.apply_remote(&[SyncNote {
            uuid4: "n1".into(),
            created_at: "2026-01-01 00:00:00".into(),
            updated_at: "0000000000002-0000-n".into(),
            ai_updated_at: "0000000000002-0000-n".into(),
            deleted: true,
            ..SyncNote::default()
        }])
        .unwrap();
        assert!(db.sent(None, 10).unwrap().is_empty());
    }

    #[test]
    fn records_without_an_id_or_a_note_are_skipped() {
        let mut db = db();
        let blank = Sent::default();
        let no_note = Sent {
            uuid4: "s1".into(),
            ..Sent::default()
        };
        assert_eq!(db.apply_remote_sent(&[blank, no_note]).unwrap(), 0);
    }
}
