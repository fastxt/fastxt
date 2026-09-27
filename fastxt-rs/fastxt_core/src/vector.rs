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

//! Note embeddings and vector search.
//!
//! Vectors live in the `embedding` table (one row per note and model) and are
//! indexed in one sqlite-vec table per vector length, `vec_<dim>`, partitioned
//! by model id and using cosine distance. Any embedding model works; vectors
//! from different models are never compared with each other.

use crate::error::{Error, Result};
use crate::model::{EmbeddingModelInfo, EmbeddingStamp, Note, NoteKey, ScoredNote, SyncEmbedding};
use crate::schema::table_exists;
use crate::store::{Fastxt, NOTE_COLUMNS, note_from_row, now_utc};
use rusqlite::{Connection, OptionalExtension, params};

/// Largest vector length accepted (sqlite-vec's limit).
pub const MAX_DIM: usize = 8192;

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn dim_of(v: i64) -> usize {
    usize::try_from(v).unwrap_or(0)
}

fn validate(v: &[f32]) -> Result<()> {
    if v.is_empty() || v.len() > MAX_DIM {
        return Err(Error::Invalid(format!(
            "embedding length {} is outside 1..={MAX_DIM}",
            v.len()
        )));
    }
    if v.iter().any(|x| !x.is_finite()) {
        return Err(Error::Invalid("embedding contains NaN or infinity".into()));
    }
    if v.iter().all(|x| *x == 0.0) {
        return Err(Error::Invalid("embedding is all zeros".into()));
    }
    Ok(())
}

fn ensure_vec_table(conn: &Connection, dim: usize) -> Result<()> {
    conn.execute_batch(&format!(
        "CREATE VIRTUAL TABLE IF NOT EXISTS vec_{dim} USING vec0(
             id INTEGER PRIMARY KEY,
             model_id TEXT PARTITION KEY,
             embedding float[{dim}] distance_metric=cosine
         )"
    ))?;
    Ok(())
}

fn delete_vec_row(conn: &Connection, id: i64, dim: usize) -> Result<()> {
    if table_exists(conn, &format!("vec_{dim}"))? {
        conn.execute(&format!("DELETE FROM vec_{dim} WHERE id = ?1"), [id])?;
    }
    Ok(())
}

/// Remove every embedding of a note (its text changed or it was deleted).
pub(crate) fn delete_embeddings(conn: &Connection, note_uuid: &str) -> Result<()> {
    let mut stmt = conn.prepare("SELECT id, dim FROM embedding WHERE note_uuid = ?1")?;
    let rows = stmt
        .query_map([note_uuid], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, dim) in rows {
        delete_vec_row(conn, id, dim_of(dim))?;
    }
    conn.execute("DELETE FROM embedding WHERE note_uuid = ?1", [note_uuid])?;
    Ok(())
}

fn store_row(
    conn: &Connection,
    note_uuid: &str,
    model_id: &str,
    note_updated_at: &str,
    vector: &[f32],
) -> Result<()> {
    validate(vector)?;
    if model_id.trim().is_empty() {
        return Err(Error::Invalid("embedding model id is empty".into()));
    }
    let dim = vector.len();
    let old: Option<(i64, i64)> = conn
        .query_row(
            "SELECT id, dim FROM embedding WHERE note_uuid = ?1 AND model_id = ?2",
            [note_uuid, model_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((id, old_dim)) = old {
        delete_vec_row(conn, id, dim_of(old_dim))?;
    }
    let id: i64 = conn.query_row(
        "INSERT INTO embedding (note_uuid, model_id, dim, vector, note_updated_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(note_uuid, model_id) DO UPDATE SET
             dim = excluded.dim, vector = excluded.vector,
             note_updated_at = excluded.note_updated_at, created_at = excluded.created_at
         RETURNING id",
        params![
            note_uuid,
            model_id,
            i64::try_from(dim).unwrap_or(0),
            to_blob(vector),
            note_updated_at,
            now_utc()
        ],
        |r| r.get(0),
    )?;
    ensure_vec_table(conn, dim)?;
    conn.execute(
        &format!("INSERT INTO vec_{dim} (id, model_id, embedding) VALUES (?1, ?2, ?3)"),
        params![id, model_id, to_blob(vector)],
    )?;
    Ok(())
}

impl Fastxt {
    /// Store an embedding computed from the note's current text.
    ///
    /// # Errors
    /// [`Error::NotFound`] for a missing note, [`Error::Invalid`] for an empty
    /// model id or an unusable vector.
    pub fn store_embedding(&mut self, key: &NoteKey, model_id: &str, vector: &[f32]) -> Result<()> {
        let note = self
            .get(key)?
            .ok_or_else(|| Error::NotFound(key.to_string()))?;
        let tx = self.conn.transaction()?;
        store_row(&tx, &note.uuid4, model_id, &note.updated_at, vector)?;
        tx.commit()?;
        Ok(())
    }

    /// Store an embedding computed from `note`, unless the note has been
    /// edited or deleted since it was read. Returns whether it was stored.
    ///
    /// # Errors
    /// See [`Fastxt::store_embedding`].
    pub fn store_embedding_if_current(
        &mut self,
        note: &Note,
        model_id: &str,
        vector: &[f32],
    ) -> Result<bool> {
        let current = self.get(&NoteKey::Rowid(note.rowid))?;
        if current.as_ref().map(|n| n.updated_at.as_str()) != Some(note.updated_at.as_str()) {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        store_row(&tx, &note.uuid4, model_id, &note.updated_at, vector)?;
        tx.commit()?;
        Ok(true)
    }

    /// A note's stored embedding for `model_id`.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn embedding(&self, key: &NoteKey, model_id: &str) -> Result<Option<Vec<f32>>> {
        let Some(note) = self.get(key)? else {
            return Ok(None);
        };
        Ok(self
            .conn
            .query_row(
                "SELECT vector FROM embedding WHERE note_uuid = ?1 AND model_id = ?2",
                [&note.uuid4, model_id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|b| from_blob(&b)))
    }

    const MISSING_EMBEDDING: &'static str = "NOT EXISTS (SELECT 1 FROM embedding e
         WHERE e.note_uuid = n.uuid4 AND e.model_id = ?1 AND e.note_updated_at = n.updated_at)";

    /// Notes with no up-to-date embedding for `model_id`, newest first.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn notes_without_embedding(&self, model_id: &str, limit: u32) -> Result<Vec<Note>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {NOTE_COLUMNS} FROM note n WHERE n.deleted = 0 AND {}
             ORDER BY n.created_at DESC, n.rowid DESC LIMIT ?2",
            Self::MISSING_EMBEDDING
        ))?;
        let notes = stmt
            .query_map(params![model_id, limit], note_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(notes)
    }

    /// How many notes have no up-to-date embedding for `model_id`.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn count_without_embedding(&self, model_id: &str) -> Result<u32> {
        Ok(self.conn.query_row(
            &format!(
                "SELECT count(*) FROM note n WHERE n.deleted = 0 AND {}",
                Self::MISSING_EMBEDDING
            ),
            [model_id],
            |r| r.get(0),
        )?)
    }

    /// Embedding models with stored vectors, most used first.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn embedding_models(&self) -> Result<Vec<EmbeddingModelInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.model_id, e.dim, count(*) FROM embedding e
             JOIN note n ON n.uuid4 = e.note_uuid AND n.deleted = 0
             GROUP BY e.model_id, e.dim ORDER BY count(*) DESC, e.model_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(EmbeddingModelInfo {
                    model_id: r.get(0)?,
                    dim: dim_of(r.get(1)?),
                    count: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Nearest notes to `query` among vectors from `model_id`, most similar
    /// first. `score` is cosine similarity (1 = same direction); results below
    /// `min_similarity` are dropped.
    ///
    /// # Errors
    /// [`Error::Invalid`] for an unusable query vector; database errors.
    pub fn semantic_search(
        &self,
        query: &[f32],
        model_id: &str,
        limit: u32,
        min_similarity: f64,
    ) -> Result<Vec<ScoredNote>> {
        validate(query)?;
        let dim = query.len();
        if limit == 0 || !table_exists(&self.conn, &format!("vec_{dim}"))? {
            return Ok(Vec::new());
        }
        // sqlite-vec needs `k = ?` on the virtual table itself, so the knn
        // runs in a CTE and the join happens afterwards.
        let mut stmt = self.conn.prepare(&format!(
            "WITH knn AS (
                 SELECT id, distance FROM vec_{dim}
                 WHERE embedding MATCH ?1 AND k = ?2 AND model_id = ?3
             )
             SELECT {NOTE_COLUMNS}, knn.distance FROM knn
             JOIN embedding e ON e.id = knn.id
             JOIN note n ON n.uuid4 = e.note_uuid
             WHERE n.deleted = 0
             ORDER BY knn.distance"
        ))?;
        let k = limit.min(4096);
        let hits = stmt
            .query_map(params![to_blob(query), k, model_id], |r| {
                let distance: f64 = r.get(10)?;
                Ok(ScoredNote {
                    note: note_from_row(r)?,
                    score: 1.0 - distance,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(hits
            .into_iter()
            .filter(|h| h.score >= min_similarity)
            .collect())
    }

    /// Notes most similar to the given note. Uses `model_id` when given,
    /// otherwise the note's most recent embedding. Empty when the note has no
    /// embedding.
    ///
    /// # Errors
    /// [`Error::NotFound`] for a missing note; database errors.
    pub fn related(
        &self,
        key: &NoteKey,
        model_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ScoredNote>> {
        let note = self
            .get(key)?
            .ok_or_else(|| Error::NotFound(key.to_string()))?;
        let found: Option<(String, Vec<u8>)> = match model_id {
            Some(m) => self
                .conn
                .query_row(
                    "SELECT model_id, vector FROM embedding WHERE note_uuid = ?1 AND model_id = ?2",
                    [&note.uuid4, m],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
            None => self
                .conn
                .query_row(
                    "SELECT model_id, vector FROM embedding WHERE note_uuid = ?1
                     ORDER BY created_at DESC LIMIT 1",
                    [&note.uuid4],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?,
        };
        let Some((model, blob)) = found else {
            return Ok(Vec::new());
        };
        let mut hits = self.semantic_search(&from_blob(&blob), &model, limit + 1, -1.0)?;
        hits.retain(|h| h.note.uuid4 != note.uuid4);
        hits.truncate(limit as usize);
        Ok(hits)
    }

    /// Recreate every `vec_<dim>` table from the `embedding` table.
    pub(crate) fn rebuild_vector_index(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        let tables: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table'
                 AND name GLOB 'vec_[0-9]*' AND sql LIKE 'CREATE VIRTUAL TABLE%'",
            )?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for t in tables {
            tx.execute_batch(&format!("DROP TABLE {t}"))?;
        }
        let rows: Vec<(i64, String, i64, Vec<u8>)> = {
            let mut stmt = tx.prepare("SELECT id, model_id, dim, vector FROM embedding")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, model_id, dim, blob) in rows {
            let dim = dim_of(dim);
            let vector = from_blob(&blob);
            if vector.len() != dim || validate(&vector).is_err() {
                tx.execute("DELETE FROM embedding WHERE id = ?1", [id])?;
                continue;
            }
            ensure_vec_table(&tx, dim)?;
            tx.execute(
                &format!("INSERT INTO vec_{dim} (id, model_id, embedding) VALUES (?1, ?2, ?3)"),
                params![id, model_id, blob],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    // ----------------------------------------------------------------------
    // Sync support
    // ----------------------------------------------------------------------

    /// Up-to-date embeddings for `model_id`, as (note, note version) pairs.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn embedding_manifest(&self, model_id: &str) -> Result<Vec<EmbeddingStamp>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.note_uuid, e.note_updated_at FROM embedding e
             JOIN note n ON n.uuid4 = e.note_uuid
             WHERE e.model_id = ?1 AND n.deleted = 0 AND e.note_updated_at = n.updated_at",
        )?;
        let rows = stmt
            .query_map([model_id], |r| {
                Ok(EmbeddingStamp {
                    note_uuid: r.get(0)?,
                    note_updated_at: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Embeddings for `model_id` of the given notes.
    ///
    /// # Errors
    /// Fails on a database error.
    pub fn embedding_records(
        &self,
        model_id: &str,
        uuids: &[String],
    ) -> Result<Vec<SyncEmbedding>> {
        let mut stmt = self.conn.prepare(
            "SELECT note_updated_at, vector FROM embedding WHERE note_uuid = ?1 AND model_id = ?2",
        )?;
        let mut out = Vec::with_capacity(uuids.len());
        for uuid in uuids {
            if let Some((version, blob)) = stmt
                .query_row(params![uuid, model_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .optional()?
            {
                out.push(SyncEmbedding {
                    note_uuid: uuid.clone(),
                    model_id: model_id.to_string(),
                    note_updated_at: version,
                    vector: from_blob(&blob),
                });
            }
        }
        Ok(out)
    }

    /// Store embeddings received from a peer. A vector is only accepted when
    /// it was computed from the same note version this device has, so a peer
    /// can never attach a vector to text it doesn't describe.
    ///
    /// # Errors
    /// Fails on a database error; invalid vectors are skipped.
    pub fn apply_remote_embeddings(&mut self, records: &[SyncEmbedding]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut stored = 0;
        for rec in records {
            let version: Option<String> = tx
                .query_row(
                    "SELECT updated_at FROM note WHERE uuid4 = ?1 AND deleted = 0",
                    [&rec.note_uuid],
                    |r| r.get(0),
                )
                .optional()?;
            if version.as_deref() != Some(rec.note_updated_at.as_str()) {
                continue;
            }
            let have: Option<String> = tx
                .query_row(
                    "SELECT note_updated_at FROM embedding WHERE note_uuid = ?1 AND model_id = ?2",
                    [&rec.note_uuid, &rec.model_id],
                    |r| r.get(0),
                )
                .optional()?;
            if have.as_deref() == Some(rec.note_updated_at.as_str()) {
                continue;
            }
            if store_row(
                &tx,
                &rec.note_uuid,
                &rec.model_id,
                &rec.note_updated_at,
                &rec.vector,
            )
            .is_ok()
            {
                stored += 1;
            }
        }
        tx.commit()?;
        Ok(stored)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::NewNote;
    use crate::model::NoteKey;
    use crate::store::Fastxt;

    fn unit(dim: usize, i: usize, scale: f32) -> Vec<f32> {
        let mut v = vec![0.0; dim];
        v[i] = scale;
        v
    }

    fn setup(dim: usize) -> (Fastxt, Vec<i64>) {
        let mut db = Fastxt::open_in_memory().unwrap();
        let rows = [
            "rust basics",
            "rust advanced",
            "python basics",
            "cooking pasta",
        ]
        .iter()
        .map(|t| db.insert(NewNote::new(*t, "")).unwrap().rowid)
        .collect::<Vec<_>>();
        let vecs = [
            unit(dim, 0, 1.0),
            // Same direction as the query but un-normalised (length 10).
            {
                let mut v = unit(dim, 0, 9.95);
                v[1] = 0.998;
                v
            },
            {
                let mut v = unit(dim, 0, 0.5);
                v[1] = 0.5;
                v
            },
            unit(dim, 2, 1.0),
        ];
        for (r, v) in rows.iter().zip(vecs.iter()) {
            db.store_embedding(&NoteKey::Rowid(*r), "test:model", v)
                .unwrap();
        }
        (db, rows)
    }

    #[test]
    fn any_dimension_works_and_uses_cosine() {
        for dim in [384, 768, 3072] {
            let (db, rows) = setup(dim);
            let hits = db
                .semantic_search(&unit(dim, 0, 1.0), "test:model", 10, 0.5)
                .unwrap();
            let order: Vec<i64> = hits.iter().map(|h| h.note.rowid).collect();
            assert_eq!(order, vec![rows[0], rows[1], rows[2]], "dim {dim}");
            assert!((hits[1].score - 0.995).abs() < 0.01, "scale doesn't matter");
        }
    }

    #[test]
    fn models_are_kept_apart() {
        let (mut db, rows) = setup(8);
        db.store_embedding(&NoteKey::Rowid(rows[3]), "other:model", &unit(8, 0, 1.0))
            .unwrap();
        let hits = db
            .semantic_search(&unit(8, 0, 1.0), "other:model", 10, -1.0)
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].note.rowid, rows[3]);
        assert_eq!(db.embedding_models().unwrap().len(), 2);
    }

    #[test]
    fn related_excludes_the_note_itself() {
        let (db, rows) = setup(8);
        let rel = db.related(&NoteKey::Rowid(rows[0]), None, 2).unwrap();
        assert_eq!(
            rel.iter().map(|h| h.note.rowid).collect::<Vec<_>>(),
            vec![rows[1], rows[2]]
        );
    }

    #[test]
    fn editing_or_deleting_a_note_drops_its_embedding() {
        let (mut db, rows) = setup(8);
        let q = unit(8, 0, 1.0);
        db.update(&NoteKey::Rowid(rows[0]), "rust basics, revised", "")
            .unwrap();
        db.delete(&NoteKey::Rowid(rows[1])).unwrap();
        let hits = db.semantic_search(&q, "test:model", 10, 0.5).unwrap();
        assert_eq!(
            hits.iter().map(|h| h.note.rowid).collect::<Vec<_>>(),
            vec![rows[2]]
        );
        assert_eq!(db.count_without_embedding("test:model").unwrap(), 1);
    }

    #[test]
    fn stale_vectors_are_not_stored() {
        let (mut db, rows) = setup(8);
        let before = db.get(&NoteKey::Rowid(rows[3])).unwrap().unwrap();
        db.update(&NoteKey::Rowid(rows[3]), "cooking pasta tonight", "")
            .unwrap();
        assert!(
            !db.store_embedding_if_current(&before, "test:model", &unit(8, 3, 1.0))
                .unwrap()
        );
    }

    #[test]
    fn rejects_unusable_vectors() {
        let (mut db, rows) = setup(8);
        let key = NoteKey::Rowid(rows[0]);
        assert!(db.store_embedding(&key, "m", &[]).is_err());
        assert!(db.store_embedding(&key, "m", &[0.0; 8]).is_err());
        assert!(db.store_embedding(&key, "m", &[f32::NAN; 8]).is_err());
    }

    #[test]
    fn remote_embeddings_need_a_matching_note_version() {
        let (a, rows) = setup(8);
        let mut b = Fastxt::open_in_memory().unwrap();
        let uuids: Vec<String> = a.manifest().unwrap().into_iter().map(|s| s.uuid4).collect();
        b.apply_remote(&a.records(&uuids).unwrap()).unwrap();
        let recs = a.embedding_records("test:model", &uuids).unwrap();
        assert_eq!(b.apply_remote_embeddings(&recs).unwrap(), 4);
        assert_eq!(
            b.apply_remote_embeddings(&recs).unwrap(),
            0,
            "already stored"
        );

        // A vector for an older version of the note is refused.
        let mut stale = recs[0].clone();
        stale.note_updated_at = "0000000000001-0000-x".into();
        let key = NoteKey::Rowid(rows[0]);
        let _ = key;
        assert_eq!(b.apply_remote_embeddings(&[stale]).unwrap(), 0);
    }

    #[test]
    fn rebuild_restores_the_index() {
        let (mut db, rows) = setup(8);
        db.rebuild_vector_index().unwrap();
        let hits = db
            .semantic_search(&unit(8, 0, 1.0), "test:model", 1, 0.0)
            .unwrap();
        assert_eq!(hits[0].note.rowid, rows[0]);
    }
}
