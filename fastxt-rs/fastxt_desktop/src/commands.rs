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

//! Framework-agnostic logic for the desktop app, on the typed core API.
//!
//! Everything here is synchronous and `Send`; the Iced layer calls it from
//! background threads. GUI types stay out.

use fastxt_core::ai::JobReport;
use fastxt_core::model::{CategoryCount, Filter, Note, Page, ScoredNote, Settings};
pub use fastxt_core::sync::client::SyncReport;
pub use fastxt_core::sync::server::ServerHandle;
use fastxt_core::{Fastxt, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Everything fetched at startup.
#[derive(Debug, Clone)]
pub struct Boot {
    pub settings: Settings,
    pub categories: Vec<(String, u32)>,
    pub list: ListOutcome,
}

/// Load settings, categories and the first page in one background job.
pub fn boot(db: &AppDb) -> Boot {
    Boot {
        settings: settings(db),
        categories: category_pairs(db),
        list: list(db, PAGE, 0, None),
    }
}

/// Page size shared by the list UI.
pub const PAGE: u32 = 50;

/// How the current list was produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListQuery {
    /// Newest notes, optionally one category.
    Browse {
        category: Option<String>,
        offset: u32,
    },
    /// Text search.
    Search { query: String, offset: u32 },
    /// Semantic search (AI; always first page).
    Semantic { query: String },
}

/// Re-run a list query.
pub fn fetch(db: &AppDb, query: &ListQuery) -> ListOutcome {
    match query {
        ListQuery::Browse { category, offset } => list(db, PAGE, *offset, category.as_deref()),
        ListQuery::Search { query, offset } => text_search(db, query, PAGE, *offset),
        ListQuery::Semantic { query } => semantic_search(db, query, PAGE),
    }
}

/// Category counts as (name, count) pairs for the sidebar.
pub fn category_pairs(db: &AppDb) -> Vec<(String, u32)> {
    categories(db)
        .into_iter()
        .map(|c| (c.category, c.count))
        .collect()
}

impl ListQuery {
    /// Current page offset (semantic search is always the first page).
    #[must_use]
    pub fn offset(&self) -> u32 {
        match self {
            ListQuery::Browse { offset, .. } | ListQuery::Search { offset, .. } => *offset,
            ListQuery::Semantic { .. } => 0,
        }
    }

    /// Same query at another offset.
    #[must_use]
    pub fn with_offset(&self, offset: u32) -> Self {
        match self {
            ListQuery::Browse { category, .. } => ListQuery::Browse {
                category: category.clone(),
                offset,
            },
            ListQuery::Search { query, .. } => ListQuery::Search {
                query: query.clone(),
                offset,
            },
            ListQuery::Semantic { .. } => self.clone(),
        }
    }
}

/// The shared database handle the app and any started sync server use.
pub type AppDb = Arc<Mutex<Fastxt>>;

/// Open the app's database (default location).
///
/// # Errors
/// See [`Fastxt::open_default`].
pub fn open_db() -> Result<Fastxt> {
    Fastxt::open_default()
}

/// A note rendered as a card.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoteCard {
    pub rowid: i64,
    pub uuid4: String,
    pub txt: String,
    pub tags: String,
    pub ai_tags: String,
    pub summary: String,
    pub category: Option<String>,
    pub created_at: String,
    /// Similarity in `[-1, 1]` when the list came from semantic search.
    pub similarity: Option<f64>,
}

impl NoteCard {
    fn from(note: &Note, similarity: Option<f64>) -> Self {
        NoteCard {
            rowid: note.rowid,
            uuid4: note.uuid4.clone(),
            txt: note.txt.clone(),
            tags: note.tags.clone(),
            ai_tags: note.ai_tag_list().join(", "),
            summary: note.ai_summary.clone().unwrap_or_default(),
            category: note.ai_category.clone(),
            created_at: note
                .created_at
                .split(' ')
                .next()
                .unwrap_or_default()
                .to_string(),
            similarity,
        }
    }
}

/// One page of results plus a headline.
#[derive(Debug, Clone, Default)]
pub struct ListOutcome {
    pub label: String,
    pub notes: Vec<NoteCard>,
    pub total: u32,
    pub offset: u32,
    pub error: Option<String>,
}

fn lock<T>(db: &AppDb, f: impl FnOnce(&mut Fastxt) -> Result<T>) -> std::result::Result<T, String> {
    let mut guard = db
        .lock()
        .map_err(|_| "the database is locked".to_string())?;
    f(&mut guard).map_err(|e| e.to_string())
}

fn page_outcome(page: Page, offset: u32, verb: &str) -> ListOutcome {
    let total = page.count;
    let label = if total == 0 {
        format!("No notes {verb}")
    } else {
        format!("{verb}: {} of {total}", page.notes.len())
    };
    ListOutcome {
        label,
        notes: page.notes.iter().map(|n| NoteCard::from(n, None)).collect(),
        total,
        offset,
        error: None,
    }
}

/// List the newest notes.
pub fn list(db: &AppDb, limit: u32, offset: u32, category: Option<&str>) -> ListOutcome {
    let filter = Filter {
        category: category.map(str::to_string),
    };
    match lock(db, |db| db.list(limit, offset, &filter)) {
        Ok(page) => page_outcome(page, offset, "listed"),
        Err(e) => ListOutcome {
            error: Some(e),
            ..Default::default()
        },
    }
}

/// Full-text search (FTS5 trigram index, LIKE fallback for short terms).
pub fn text_search(db: &AppDb, query: &str, limit: u32, offset: u32) -> ListOutcome {
    match lock(db, |db| db.search(query, limit, offset)) {
        Ok(page) => page_outcome(page, offset, "found"),
        Err(e) => ListOutcome {
            error: Some(e),
            ..Default::default()
        },
    }
}

/// Semantic search; scores notes by meaning via the configured embedder.
/// Falls back to text search when AI is unavailable, with a note why.
pub fn semantic_search(db: &AppDb, query: &str, limit: u32) -> ListOutcome {
    let result = lock(db, |db| {
        let ai = fastxt_core::ai::Ai::for_db(db)?;
        let check = ai.check();
        if !check.ok {
            return Err(fastxt_core::Error::AiUnavailable(check.message));
        }
        let vector = ai.embed(query)?;
        db.semantic_search(&vector, ai.embedding_model_id(), limit, 0.0)
    });
    match result {
        Ok(hits) => ListOutcome {
            label: format!("{} most similar notes", hits.len()),
            notes: hits
                .iter()
                .map(|h: &ScoredNote| NoteCard::from(&h.note, Some(h.score)))
                .collect(),
            total: hits.len() as u32,
            offset: 0,
            error: None,
        },
        Err(e) => {
            // Degrade to text search but say so.
            let fallback = text_search(db, query, limit, 0);
            ListOutcome {
                label: format!("{} (AI unavailable — text results)", fallback.label),
                error: Some(e),
                ..fallback
            }
        }
    }
}

/// Fetch one note for the detail editor.
pub fn get_note(db: &AppDb, rowid: i64) -> Option<NoteCard> {
    lock(db, |db| db.get(&rowid.into()))
        .ok()
        .flatten()
        .map(|n| NoteCard::from(&n, None))
}

/// Save a new note; returns its rowid.
pub fn insert_note(db: &AppDb, txt: &str, tags: &str) -> std::result::Result<i64, String> {
    lock(db, |db| {
        db.insert(fastxt_core::model::NewNote::new(txt, tags))
            .map(|n| n.rowid)
    })
}

/// Save a new note together with an AI summary generated for it, in one step.
pub fn insert_with_summary(
    db: &AppDb,
    txt: &str,
    tags: &str,
    summary: &str,
) -> std::result::Result<i64, String> {
    lock(db, |db| {
        let mut new = fastxt_core::model::NewNote::new(txt, tags);
        new.ai_summary = Some(summary.to_string());
        db.insert(new).map(|n| n.rowid)
    })
}

/// Change a note's text and tags.
pub fn update_note(
    db: &AppDb,
    rowid: i64,
    txt: &str,
    tags: &str,
) -> std::result::Result<(), String> {
    lock(db, |db| db.update(&rowid.into(), txt, tags).map(|_| ()))
}

/// Delete a note (tombstone; syncs to peers).
pub fn delete_note(db: &AppDb, rowid: i64) -> std::result::Result<(), String> {
    lock(db, |db| db.delete(&rowid.into()).map(|_| ()))
}

/// AI tag suggestions for arbitrary text; `Err` explains what's missing.
pub fn ai_tags(db: &AppDb, text: &str) -> std::result::Result<Vec<String>, String> {
    lock(db, |db| {
        let vocabulary: Vec<String> = db.tag_vocabulary(40)?.into_iter().map(|t| t.tag).collect();
        fastxt_core::ai::Ai::for_db(db)?.suggest_tags(text, &vocabulary)
    })
    .map_err(|e| e.to_string())
}

/// AI summary of arbitrary text — nothing is saved.
pub fn ai_summarize(db: &AppDb, text: &str) -> std::result::Result<String, String> {
    lock(db, |db| fastxt_core::ai::summarize_text(db, text)).map_err(|e| e.to_string())
}

/// Probe the AI backend; the message explains what to fix.
pub fn ai_check(db: &AppDb) -> String {
    match lock(db, |db| {
        fastxt_core::ai::Ai::for_db(db).map(|ai| ai.check())
    }) {
        Ok(check) if check.ok => check.message,
        Ok(check) => format!("✗ {}", check.message),
        Err(e) => format!("✗ {e}"),
    }
}

/// Stored settings.
pub fn settings(db: &AppDb) -> Settings {
    lock(db, |db| db.settings()).unwrap_or_default()
}

/// Persist settings.
pub fn save_settings(db: &AppDb, settings: &Settings) -> std::result::Result<(), String> {
    lock(db, |db| db.save_settings(settings).map(|_| ()))
}

/// Categories with counts.
pub fn categories(db: &AppDb) -> Vec<CategoryCount> {
    lock(db, |db| db.categories()).unwrap_or_default()
}

/// Rename a category on every note that has it.
pub fn rename_category(db: &AppDb, old: &str, new: &str) -> std::result::Result<usize, String> {
    lock(db, |db| db.rename_category(old, new))
}

/// Clear a category from every note.
pub fn dismiss_category(db: &AppDb, category: &str) -> std::result::Result<usize, String> {
    lock(db, |db| db.dismiss_category(category))
}

// ---------------------------------------------------------------------------
// Batch jobs (chunked so the UI can show progress and cancel)
// ---------------------------------------------------------------------------

/// What a batch step should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchKind {
    Tag,
    Embed,
    Organize,
}

impl BatchKind {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            BatchKind::Tag => "Tagging",
            BatchKind::Embed => "Embedding",
            BatchKind::Organize => "Organizing",
        }
    }
}

/// Run a batch job in chunks until drained or cancelled. `progress` sees the
/// running totals. Returns the accumulated report.
///
/// # Errors
/// Returns the first unrecoverable error (e.g. AI unavailable).
pub fn run_batch(
    db: &AppDb,
    kind: BatchKind,
    chunk: u32,
    cancel: &AtomicBool,
    progress: &dyn Fn(u32, u32),
) -> std::result::Result<(u32, u32), String> {
    let mut done = 0u32;
    let mut errors = 0u32;
    loop {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let silent = |_, _| {};
        let report: JobReport = lock(db, |db| match kind {
            BatchKind::Tag => fastxt_core::ai::tag_notes(db, chunk, &silent),
            BatchKind::Embed => fastxt_core::ai::embed_notes(db, chunk, &silent),
            BatchKind::Organize => fastxt_core::ai::categorize_notes(db, chunk, &silent),
        })
        .map_err(|e| e.to_string())?;
        if report.processed == 0 {
            // Drained, or the remaining notes keep failing: don't spin.
            errors += report.errors;
            break;
        }
        done += report.processed;
        errors += report.errors;
        progress(done, done + errors);
    }
    progress(done, done + errors);
    Ok((done, errors))
}

// ---------------------------------------------------------------------------
// Sync
// ---------------------------------------------------------------------------

/// Start the sync server; the pairing code is in `handle.pairing_code`.
pub fn start_server(db: &AppDb, port: u16) -> std::result::Result<ServerHandle, String> {
    let path = lock(db, |db| {
        db.path().map(std::path::Path::to_path_buf).ok_or_else(|| {
            fastxt_core::Error::Sync("the server needs a file-backed database".into())
        })
    })?;
    let server_db: AppDb = Arc::new(Mutex::new(Fastxt::open(path).map_err(|e| e.to_string())?));
    fastxt_core::sync::server::serve(server_db, port).map_err(|e| e.to_string())
}

/// Stop the sync server.
pub fn stop_server(handle: &ServerHandle) {
    handle.stop();
}

/// Sync with the server named in a pairing code.
pub fn sync(db: &AppDb, pairing_code: &str) -> std::result::Result<SyncReport, String> {
    lock(db, |db| fastxt_core::sync::client::sync(pairing_code, db)).map_err(|e| e.to_string())
}

/// Human line for a sync report.
#[must_use]
pub fn sync_report_text(report: &SyncReport) -> String {
    format!(
        "✓ synced: {} notes pulled, {} pushed, {} embeddings pulled, {} pushed",
        report.notes_pulled,
        report.notes_pushed,
        report.embeddings_pulled,
        report.embeddings_pushed
    )
}
