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

//! The Iced desktop GUI. View layer only: all work happens in
//! [`crate::commands`] on background threads.

use crate::commands::{self, AppDb, BatchKind, ListQuery, NoteCard};
use iced::futures::StreamExt;
use iced::widget::{
    QRCode, Space, button, column, container, pick_list, qr_code, row, scrollable, text,
    text_editor, text_input,
};
use iced::{Element, Length, Subscription, Task};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// The AI backends offered in settings.
const BACKENDS: [&str; 4] = ["ollama", "llamacpp", "foundry-local", "openai"];

/// Which screen the right-hand panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightPanel {
    Form,
    Detail,
    Sync,
    AiSettings,
}

/// Wrapper making `qr_code::Data` `Send` (created and used on the GUI thread).
struct SendableQrData(qr_code::Data);
// SAFETY: the QR payload is created and rendered only on the GUI thread; the
// wrapper exists solely to satisfy Iced's `Send` bound on app state.
unsafe impl Send for SendableQrData {}

/// A background batch job in flight.
struct BatchState {
    kind: BatchKind,
    done: u32,
    total: u32,
    cancel: Arc<AtomicBool>,
}

/// Top-level application state.
struct Fastxt {
    db: AppDb,

    // Create form.
    content: text_editor::Content,
    tags: String,
    ai_suggested_tags: Vec<String>,
    ai_summary: String,
    /// True while `ai_summary` was generated from the form's current text.
    ai_summary_matches_form: bool,
    status: String,
    save_busy: bool,

    // Detail / edit.
    editing: Option<(i64, text_editor::Content, String)>,
    detail_busy: bool,

    // Search + list.
    query: String,
    semantic: bool,
    list_query: ListQuery,
    list_label: String,
    notes: Vec<NoteCard>,
    total: u32,
    list_busy: bool,
    list_error: Option<String>,
    categories: Vec<(String, u32)>,
    show_categories: bool,
    rename_to: String,

    // Navigation.
    current_view: RightPanel,

    // AI settings.
    backend: &'static str,
    endpoint: String,
    model: String,
    embedding_model: String,
    timeout: String,
    settings_busy: bool,

    // Batch.
    batch: Option<BatchState>,

    // Sync.
    server: Option<commands::ServerHandle>,
    server_qr: Option<SendableQrData>,
    pairing_input: String,
    sync_status: String,
    sync_busy: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Booted(Box<commands::Boot>),

    // Navigation.
    ShowForm,
    ShowSync,
    ShowAiSettings,
    ToggleCategories,
    ToggleSemantic,

    // Create form.
    EditContent(text_editor::Action),
    TagsChanged(String),
    ClearForm,
    Save,
    Saved(Result<i64, String>),
    RequestAiTags,
    AiTagsReady(Result<Vec<String>, String>),
    UseAiTags,
    ReplaceAiTags,
    RequestSummary,
    SummaryReady(Result<String, String>),
    DismissSummary,

    // Search + list.
    QueryChanged(String),
    Search,
    SearchReady(commands::ListOutcome),
    OpenNote(i64),
    NoteOpened(Option<NoteCard>),
    PrevPage,
    NextPage,
    CategoriesReady(Vec<(String, u32)>),
    PickCategory(Option<String>),
    RenameToChanged(String),
    RenameCategory(Option<String>),
    CategoryRenamed(Result<usize, String>),
    DismissCategory(String),
    CategoryDismissed(Result<usize, String>),

    // Detail.
    EditBody(text_editor::Action),
    EditTagsChanged(String),
    SaveEdit,
    EditSaved(Result<(), String>),
    DeleteNote,
    NoteDeleted(Result<(), String>),
    CloseDetail,

    // AI settings.
    BackendPicked(&'static str),
    EndpointChanged(String),
    ModelChanged(String),
    EmbeddingModelChanged(String),
    TimeoutChanged(String),
    SaveSettings,
    SettingsSaved(Result<(), String>),
    TestConnection,
    TestConnectionReady(String),

    // Batch.
    StartBatch(BatchKind),
    BatchProgress(BatchKind, u32, u32),
    BatchDone(BatchKind, Result<(u32, u32), String>),
    CancelBatch,

    // Sync.
    StartServer,
    ServerStarted(Result<commands::ServerHandle, String>),
    StopServer,
    PairingInputChanged(String),
    Sync,
    SyncReady(Result<String, String>),
}

fn run_blocking<T, F>(f: F, msg: fn(T) -> Message) -> Task<Message>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(f)
                .await
                .expect("blocking job panicked")
        },
        msg,
    )
}

// The batch subscription's parameters. `Subscription::run` takes a plain
// function pointer, so the current job parks here for the lifetime of the
// subscription (exactly one batch runs at a time).
static BATCH: std::sync::Mutex<Option<(AppDb, Arc<AtomicBool>, BatchKind)>> =
    std::sync::Mutex::new(None);

/// Stream for the running batch job: progress, then the final report.
fn batch_stream() -> impl iced::futures::Stream<Item = Message> {
    let Some((db, cancel, kind)) = BATCH.lock().ok().and_then(|slot| slot.clone()) else {
        return iced::futures::stream::empty().boxed();
    };
    iced::stream::channel(
        100,
        move |mut sender: iced::futures::channel::mpsc::Sender<Message>| async move {
            let progress_sender = std::sync::Mutex::new(sender.clone());
            let result = tokio::task::spawn_blocking(move || {
                // futures' mpsc try_send needs &mut, so share via a mutex.
                let progress = move |done: u32, total: u32| {
                    if let Ok(mut tx) = progress_sender.lock() {
                        let _ = tx.try_send(Message::BatchProgress(kind, done, total));
                    }
                };
                commands::run_batch(&db, kind, 20, &cancel, &progress)
            })
            .await
            .unwrap_or_else(|e| Err(format!("the batch panicked: {e}")));
            let _ = sender.try_send(Message::BatchDone(kind, result));
        },
    )
    .boxed()
}

impl Fastxt {
    fn fetch_task(&self, query: ListQuery) -> Task<Message> {
        let db = self.db.clone();
        run_blocking(move || commands::fetch(&db, &query), Message::SearchReady)
    }

    fn categories_task(&self) -> Task<Message> {
        let db = self.db.clone();
        run_blocking(
            move || commands::category_pairs(&db),
            Message::CategoriesReady,
        )
    }
}

/// Launch the Fastxt desktop application.
///
/// # Errors
/// Returns an error if the Iced runtime fails to start.
pub fn run() -> iced::Result {
    iced::application(Fastxt::new, Fastxt::update, Fastxt::view)
        .title("Fastxt")
        .window(iced::window::Settings {
            size: iced::Size::new(1150.0, 760.0),
            ..Default::default()
        })
        .subscription(Fastxt::subscription)
        .run()
}

impl Fastxt {
    fn new() -> (Self, Task<Message>) {
        let db = match commands::open_db() {
            Ok(db) => Arc::new(std::sync::Mutex::new(db)),
            Err(e) => {
                eprintln!("cannot open the database: {e}");
                std::process::exit(1);
            }
        };
        let state = Fastxt {
            db: db.clone(),
            content: text_editor::Content::new(),
            tags: String::new(),
            ai_suggested_tags: Vec::new(),
            ai_summary: String::new(),
            ai_summary_matches_form: false,
            status: String::new(),
            save_busy: false,
            editing: None,
            detail_busy: false,
            query: String::new(),
            semantic: false,
            list_query: ListQuery::Browse {
                category: None,
                offset: 0,
            },
            list_label: String::new(),
            notes: Vec::new(),
            total: 0,
            list_busy: false,
            list_error: None,
            categories: Vec::new(),
            show_categories: false,
            rename_to: String::new(),
            current_view: RightPanel::Form,
            backend: "ollama",
            endpoint: String::new(),
            model: String::new(),
            embedding_model: String::new(),
            timeout: String::new(),
            settings_busy: false,
            batch: None,
            server: None,
            server_qr: None,
            pairing_input: String::new(),
            sync_status: String::new(),
            sync_busy: false,
        };
        let boot = run_blocking(move || Box::new(commands::boot(&db)), Message::Booted);
        (state, boot)
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Booted(boot) => {
                self.backend = BACKENDS
                    .iter()
                    .find(|b| **b == boot.settings.ai.backend)
                    .copied()
                    .unwrap_or("ollama");
                self.endpoint = boot.settings.ai.endpoint.clone();
                self.model = boot.settings.ai.model.clone();
                self.embedding_model = boot.settings.ai.embedding_model.clone();
                self.timeout = boot.settings.ai.timeout_secs.to_string();
                self.categories = boot.categories;
                self.list_label = boot.list.label.clone();
                self.notes = boot.list.notes.clone();
                self.total = boot.list.total;
                Task::none()
            }

            // ---- navigation ----
            Message::ShowForm => {
                self.current_view = RightPanel::Form;
                Task::none()
            }
            Message::ShowSync => {
                self.current_view = RightPanel::Sync;
                Task::none()
            }
            Message::ShowAiSettings => {
                self.current_view = RightPanel::AiSettings;
                Task::none()
            }
            Message::ToggleCategories => {
                self.show_categories = !self.show_categories;
                if self.show_categories && self.categories.is_empty() {
                    return self.categories_task();
                }
                Task::none()
            }
            Message::ToggleSemantic => {
                self.semantic = !self.semantic;
                Task::none()
            }

            // ---- create form ----
            Message::EditContent(action) => {
                self.content.perform(action);
                Task::none()
            }
            Message::TagsChanged(s) => {
                self.tags = s;
                Task::none()
            }
            Message::ClearForm => {
                self.content = text_editor::Content::new();
                self.tags.clear();
                self.ai_suggested_tags.clear();
                self.ai_summary.clear();
                self.status.clear();
                Task::none()
            }
            Message::Save => {
                let content = self.content.text();
                if content.trim().is_empty() {
                    self.status = "Enter some text first".into();
                    return Task::none();
                }
                // An AI summary generated before saving is stored with the note.
                let summary = self.ai_summary.clone();
                let summary_is_for_this_text = !summary.is_empty() && self.ai_summary_matches_form;
                let tags = self.tags.clone();
                self.save_busy = true;
                self.status = "Saving…".into();
                let db = self.db.clone();
                run_blocking(
                    move || {
                        if summary_is_for_this_text {
                            commands::insert_with_summary(&db, &content, &tags, &summary)
                        } else {
                            commands::insert_note(&db, &content, &tags)
                        }
                    },
                    Message::Saved,
                )
            }
            Message::Saved(result) => {
                self.save_busy = false;
                match result {
                    Ok(_) => {
                        self.content = text_editor::Content::new();
                        self.tags.clear();
                        self.ai_suggested_tags.clear();
                        self.ai_summary.clear();
                        self.ai_summary_matches_form = false;
                        self.status = "Saved".into();
                        let query = fresh_query(&self.list_query);
                        self.list_query = query.clone();
                        self.fetch_task(query)
                    }
                    Err(e) => {
                        self.status = format!("Save failed: {e}");
                        Task::none()
                    }
                }
            }
            Message::RequestAiTags => {
                let content = self.content.text();
                if content.trim().is_empty() {
                    self.status = "Enter some text first".into();
                    return Task::none();
                }
                self.status = "Getting AI suggestions…".into();
                let db = self.db.clone();
                run_blocking(
                    move || commands::ai_tags(&db, &content),
                    Message::AiTagsReady,
                )
            }
            Message::AiTagsReady(result) => match result {
                Ok(tags) => {
                    self.ai_suggested_tags = tags;
                    self.status.clear();
                    Task::none()
                }
                Err(e) => {
                    self.ai_suggested_tags.clear();
                    self.status = format!("AI unavailable: {e}");
                    Task::none()
                }
            },
            Message::UseAiTags => {
                let extra = self.ai_suggested_tags.join(",");
                self.tags = if self.tags.is_empty() {
                    extra
                } else {
                    format!("{},{}", self.tags, extra)
                };
                self.ai_suggested_tags.clear();
                Task::none()
            }
            Message::ReplaceAiTags => {
                self.tags = self.ai_suggested_tags.join(",");
                self.ai_suggested_tags.clear();
                Task::none()
            }
            Message::RequestSummary => {
                let content = self.content.text();
                if content.trim().is_empty() {
                    self.status = "Enter some text first".into();
                    return Task::none();
                }
                self.status = "Generating summary… (not saved until you Save)".into();
                let db = self.db.clone();
                run_blocking(
                    move || commands::ai_summarize(&db, &content),
                    Message::SummaryReady,
                )
            }
            Message::SummaryReady(result) => {
                self.ai_summary_matches_form = true;
                match result {
                    Ok(summary) => {
                        self.ai_summary = summary;
                        self.status.clear();
                        Task::none()
                    }
                    Err(e) => {
                        self.ai_summary.clear();
                        self.ai_summary_matches_form = false;
                        self.status = format!("AI unavailable: {e}");
                        Task::none()
                    }
                }
            }
            Message::DismissSummary => {
                self.ai_summary.clear();
                self.ai_summary_matches_form = false;
                Task::none()
            }

            // ---- search + list ----
            Message::QueryChanged(s) => {
                self.query = s;
                Task::none()
            }
            Message::Search => {
                if self.query.trim().is_empty() {
                    // Empty query: browse.
                    let query = ListQuery::Browse {
                        category: None,
                        offset: 0,
                    };
                    self.list_query = query.clone();
                    self.list_busy = true;
                    return self.fetch_task(query);
                }
                let query = if self.semantic {
                    ListQuery::Semantic {
                        query: self.query.clone(),
                    }
                } else {
                    ListQuery::Search {
                        query: self.query.clone(),
                        offset: 0,
                    }
                };
                self.list_query = query.clone();
                self.list_busy = true;
                if let ListQuery::Semantic { .. } = query {
                    self.status = "Searching…".into();
                }
                self.fetch_task(query)
            }
            Message::SearchReady(outcome) => {
                self.list_busy = false;
                self.list_label = outcome.label;
                self.notes = outcome.notes;
                self.total = outcome.total;
                self.list_error = outcome.error;
                if let Some(err) = &self.list_error {
                    self.status = err.clone();
                } else if self.status == "Searching…" {
                    self.status.clear();
                }
                Task::none()
            }
            Message::OpenNote(rowid) => {
                self.detail_busy = true;
                let db = self.db.clone();
                run_blocking(move || commands::get_note(&db, rowid), Message::NoteOpened)
            }
            Message::NoteOpened(card) => {
                self.detail_busy = false;
                if let Some(card) = card {
                    self.editing = Some((
                        card.rowid,
                        text_editor::Content::with_text(&card.txt),
                        card.tags,
                    ));
                    self.current_view = RightPanel::Detail;
                }
                Task::none()
            }
            Message::PrevPage | Message::NextPage => {
                let delta = if matches!(message, Message::PrevPage) {
                    self.list_query.offset().saturating_sub(commands::PAGE)
                } else {
                    self.list_query.offset() + commands::PAGE
                };
                if delta >= self.total && matches!(message, Message::NextPage) {
                    return Task::none();
                }
                self.list_query = self.list_query.with_offset(delta);
                self.list_busy = true;
                let q = self.list_query.clone();
                self.fetch_task(q)
            }
            Message::CategoriesReady(categories) => {
                self.categories = categories;
                Task::none()
            }
            Message::PickCategory(category) => {
                let query = ListQuery::Browse {
                    category: category.clone(),
                    offset: 0,
                };
                self.list_query = query.clone();
                self.list_busy = true;
                self.fetch_task(query)
            }
            Message::RenameToChanged(s) => {
                self.rename_to = s;
                Task::none()
            }
            Message::RenameCategory(category) => {
                let Some(from) = category.filter(|c| !c.is_empty()) else {
                    return Task::none();
                };
                if self.rename_to.trim().is_empty() {
                    return Task::none();
                }
                let to = self.rename_to.trim().to_string();
                self.rename_to.clear();
                let db = self.db.clone();
                run_blocking(
                    move || commands::rename_category(&db, &from, &to),
                    Message::CategoryRenamed,
                )
            }
            Message::CategoryRenamed(result) => match result {
                Ok(n) => {
                    self.status = format!("Renamed {n} notes");
                    self.categories_task()
                }
                Err(e) => {
                    self.status = format!("Rename failed: {e}");
                    Task::none()
                }
            },
            Message::DismissCategory(category) => {
                let db = self.db.clone();
                run_blocking(
                    move || commands::dismiss_category(&db, &category),
                    Message::CategoryDismissed,
                )
            }
            Message::CategoryDismissed(result) => match result {
                Ok(n) => {
                    self.status = format!("Dismissed category on {n} notes");
                    self.categories_task()
                }
                Err(e) => {
                    self.status = format!("Dismiss failed: {e}");
                    Task::none()
                }
            },

            // ---- detail ----
            Message::EditBody(action) => {
                if let Some((_, content, _)) = &mut self.editing {
                    content.perform(action);
                }
                Task::none()
            }
            Message::EditTagsChanged(s) => {
                if let Some((_, _, tags)) = &mut self.editing {
                    *tags = s;
                }
                Task::none()
            }
            Message::SaveEdit => {
                let Some((rowid, content, tags)) = &self.editing else {
                    return Task::none();
                };
                let txt = content.text();
                if txt.trim().is_empty() {
                    return Task::none();
                }
                let rowid = *rowid;
                let tags = tags.clone();
                self.detail_busy = true;
                let db = self.db.clone();
                run_blocking(
                    move || commands::update_note(&db, rowid, &txt, &tags),
                    Message::EditSaved,
                )
            }
            Message::EditSaved(result) => {
                self.detail_busy = false;
                match result {
                    Ok(()) => {
                        self.editing = None;
                        self.current_view = RightPanel::Form;
                        let query = fresh_query(&self.list_query);
                        self.list_query = query.clone();
                        self.fetch_task(query)
                    }
                    Err(e) => {
                        self.status = format!("Save failed: {e}");
                        Task::none()
                    }
                }
            }
            Message::DeleteNote => {
                let Some((rowid, _, _)) = &self.editing else {
                    return Task::none();
                };
                let rowid = *rowid;
                self.detail_busy = true;
                let db = self.db.clone();
                run_blocking(
                    move || commands::delete_note(&db, rowid),
                    Message::NoteDeleted,
                )
            }
            Message::NoteDeleted(result) => {
                self.detail_busy = false;
                self.editing = None;
                self.current_view = RightPanel::Form;
                match result {
                    Ok(()) => {
                        let query = fresh_query(&self.list_query);
                        self.list_query = query.clone();
                        let refresh = self.fetch_task(query);
                        let cats = self.categories_task();
                        refresh.chain(cats)
                    }
                    Err(e) => {
                        self.status = format!("Delete failed: {e}");
                        Task::none()
                    }
                }
            }
            Message::CloseDetail => {
                self.editing = None;
                self.current_view = RightPanel::Form;
                Task::none()
            }

            // ---- AI settings ----
            Message::BackendPicked(b) => {
                self.backend = b;
                Task::none()
            }
            Message::EndpointChanged(s) => {
                self.endpoint = s;
                Task::none()
            }
            Message::ModelChanged(s) => {
                self.model = s;
                Task::none()
            }
            Message::EmbeddingModelChanged(s) => {
                self.embedding_model = s;
                Task::none()
            }
            Message::TimeoutChanged(s) => {
                self.timeout = s;
                Task::none()
            }
            Message::SaveSettings => {
                let mut settings = fastxt_core::Settings::default();
                settings.ai.backend = self.backend.to_string();
                settings.ai.endpoint = self.endpoint.trim().to_string();
                settings.ai.model = self.model.trim().to_string();
                settings.ai.embedding_model = self.embedding_model.trim().to_string();
                settings.ai.timeout_secs = self.timeout.trim().parse().unwrap_or(60);
                self.settings_busy = true;
                let db = self.db.clone();
                run_blocking(
                    move || commands::save_settings(&db, &settings),
                    Message::SettingsSaved,
                )
            }
            Message::SettingsSaved(result) => {
                self.settings_busy = false;
                self.status = match result {
                    Ok(()) => "Settings saved".into(),
                    Err(e) => format!("Could not save settings: {e}"),
                };
                Task::none()
            }
            Message::TestConnection => {
                self.settings_busy = true;
                self.status = "Testing…".into();
                let db = self.db.clone();
                run_blocking(
                    move || commands::ai_check(&db),
                    Message::TestConnectionReady,
                )
            }
            Message::TestConnectionReady(message) => {
                self.settings_busy = false;
                self.status = message;
                Task::none()
            }

            // ---- batch ----
            Message::StartBatch(kind) => {
                if self.batch.is_some() {
                    return Task::none();
                }
                let cancel = Arc::new(AtomicBool::new(false));
                if let Ok(mut slot) = BATCH.lock() {
                    *slot = Some((self.db.clone(), cancel.clone(), kind));
                }
                self.batch = Some(BatchState {
                    kind,
                    done: 0,
                    total: 0,
                    cancel,
                });
                Task::none() // subscription picks it up
            }
            Message::BatchProgress(kind, done, total) => {
                if let Some(state) = &mut self.batch
                    && state.kind == kind
                {
                    state.done = done;
                    state.total = total;
                }
                Task::none()
            }
            Message::BatchDone(kind, result) => {
                if let Ok(mut slot) = BATCH.lock() {
                    *slot = None;
                }
                if let Some(state) = &self.batch
                    && state.kind == kind
                {
                    self.batch = None;
                }
                match result {
                    Ok((done, errors)) => {
                        self.status = format!("{kind:?} done: {done} processed, {errors} errors");
                        self.categories_task()
                            .chain(self.fetch_refresh_after_batch())
                    }
                    Err(e) => {
                        self.status = format!("{kind:?} failed: {e}");
                        Task::none()
                    }
                }
            }
            Message::CancelBatch => {
                if let Some(state) = &self.batch {
                    state
                        .cancel
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Task::none()
            }

            // ---- sync ----
            Message::StartServer => {
                if self.server.is_some() {
                    return Task::none();
                }
                self.sync_status = "Starting server…".into();
                let db = self.db.clone();
                run_blocking(
                    move || commands::start_server(&db, fastxt_core::sync::DEFAULT_PORT),
                    Message::ServerStarted,
                )
            }
            Message::ServerStarted(result) => match result {
                Ok(handle) => {
                    self.server_qr = qr_code::Data::new(handle.pairing_code.as_bytes())
                        .ok()
                        .map(SendableQrData);
                    self.server = Some(handle);
                    self.sync_status.clear();
                    Task::none()
                }
                Err(e) => {
                    self.sync_status = format!("Could not start the server: {e}");
                    Task::none()
                }
            },
            Message::StopServer => {
                if let Some(handle) = self.server.take() {
                    commands::stop_server(&handle);
                }
                self.server_qr = None;
                self.sync_status.clear();
                Task::none()
            }
            Message::PairingInputChanged(s) => {
                self.pairing_input = s;
                Task::none()
            }
            Message::Sync => {
                if self.pairing_input.trim().is_empty() {
                    return Task::none();
                }
                self.sync_busy = true;
                self.sync_status = "Syncing…".into();
                let code = self.pairing_input.trim().to_string();
                let db = self.db.clone();
                run_blocking(
                    move || commands::sync(&db, &code).map(|r| commands::sync_report_text(&r)),
                    Message::SyncReady,
                )
            }
            Message::SyncReady(result) => {
                self.sync_busy = false;
                self.sync_status = result.unwrap_or_else(|e| format!("✗ {e}"));
                let query = fresh_query(&self.list_query);
                self.list_query = query.clone();
                self.fetch_task(query)
            }
        }
    }

    fn fetch_refresh_after_batch(&self) -> Task<Message> {
        let query = fresh_query(&self.list_query);
        self.fetch_task(query)
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.batch.is_none() {
            return Subscription::none();
        }
        Subscription::run(batch_stream)
    }

    fn view(&self) -> Element<'_, Message> {
        let left = container(self.left_panel())
            .width(Length::FillPortion(2))
            .padding(10);
        let right = container(self.right_panel())
            .width(Length::FillPortion(3))
            .padding(10);
        row![left, right].into()
    }

    // ---- left panel ----

    fn left_panel(&self) -> Element<'_, Message> {
        let header = row![
            text("Fastxt").size(18),
            Space::new().width(Length::Fill),
            button(text("AI")).on_press(Message::ShowAiSettings),
            button(text("Sync")).on_press(Message::ShowSync),
            button(text("Create")).on_press(Message::ShowForm),
        ]
        .spacing(6);

        let search_row = row![
            text_input("search…", &self.query)
                .on_input(Message::QueryChanged)
                .on_submit(Message::Search),
            button(text(if self.semantic {
                "🔎 Semantic ✓"
            } else {
                "🔎 Semantic"
            }))
            .on_press(Message::ToggleSemantic),
            button(text("Go")).on_press_maybe((!self.list_busy).then_some(Message::Search)),
            button(text(if self.show_categories {
                "📂 Hide"
            } else {
                "📂 Categories"
            }))
            .on_press(Message::ToggleCategories),
        ]
        .spacing(6);

        let mut col = column![header, search_row].spacing(10);
        if let ListQuery::Semantic { query } = &self.list_query {
            col = col.push(text(format!("Semantic results for “{query}”")).size(11));
        }
        if let Some(err) = &self.list_error {
            col = col.push(text(err).size(11));
        }
        if self.show_categories {
            col = col.push(self.categories_panel());
        }
        col = col.push(self.notes_list());
        col.into()
    }

    fn categories_panel(&self) -> Element<'_, Message> {
        let mut col = column![text("Categories").size(13)].spacing(4);
        if self.categories.is_empty() {
            col = col.push(text("None yet — run Organize in AI settings").size(11));
        }
        let active = match &self.list_query {
            ListQuery::Browse { category, .. } => category.clone(),
            _ => None,
        };
        for (name, count) in &self.categories {
            let marker = if active.as_deref() == Some(name.as_str()) {
                format!("{name} ({count}) ✓")
            } else {
                format!("{name} ({count})")
            };
            col = col.push(
                row![
                    button(text(marker).size(12))
                        .on_press(Message::PickCategory(Some(name.clone()))),
                    Space::new().width(Length::Fill),
                    button(text("✎").size(12))
                        .on_press(Message::RenameCategory(Some(name.clone()))),
                    button(text("×").size(12)).on_press(Message::DismissCategory(name.clone())),
                ]
                .spacing(4),
            );
        }
        if !self.categories.is_empty() {
            col = col.push(text("Rename selected category to:").size(11));
            col = col.push(
                row![
                    text_input("new name…", &self.rename_to)
                        .on_input(Message::RenameToChanged)
                        .on_submit(Message::RenameCategory(active.clone())),
                    button(text("Rename")).on_press(Message::RenameCategory(active.clone())),
                ]
                .spacing(6),
            );
        }
        if active.is_some() {
            col = col.push(button(text("show all notes")).on_press(Message::PickCategory(None)));
        }
        col.into()
    }

    fn notes_list(&self) -> Element<'_, Message> {
        let offset = self.list_query.offset();
        let prev_enabled = offset > 0;
        let next_enabled = offset + commands::PAGE < self.total;
        let mut col = column![]
            .spacing(8)
            .push(text(self.list_label.as_str()).size(12));
        if self.notes.is_empty() && !self.list_busy {
            col = col.push(text("No notes yet — create one on the right").size(12));
        }
        for note in &self.notes {
            col = col.push(note_card(note));
        }
        col = col.push(
            row![
                button(text("‹ Prev")).on_press_maybe(prev_enabled.then_some(Message::PrevPage)),
                text(format!(
                    "{}–{} of {}",
                    offset + 1,
                    (offset + self.notes.len() as u32).min(self.total),
                    self.total
                ))
                .size(11),
                button(text("Next ›")).on_press_maybe(next_enabled.then_some(Message::NextPage)),
            ]
            .spacing(6),
        );
        scrollable(col).height(Length::Fill).into()
    }

    // ---- right panel ----

    fn right_panel(&self) -> Element<'_, Message> {
        if let Some((_, content, tags)) = &self.editing {
            return self.detail_page(content, tags);
        }
        match self.current_view {
            RightPanel::Form => self.form_page(),
            RightPanel::Sync => self.sync_page(),
            RightPanel::AiSettings => self.ai_settings_page(),
            RightPanel::Detail => self.form_page(),
        }
    }

    fn form_page(&self) -> Element<'_, Message> {
        let actions = row![
            button(text("Clear")).on_press(Message::ClearForm),
            button(text("Save")).on_press_maybe((!self.save_busy).then_some(Message::Save)),
            button(text("AI Tags")).on_press(Message::RequestAiTags),
            button(text("Summarize")).on_press(Message::RequestSummary),
        ]
        .spacing(6);

        let mut col = column![actions].spacing(10);

        if !self.ai_summary.is_empty() {
            col = col.push(
                row![
                    text("Summary (saved with the note when you Save):").size(11),
                    Space::new().width(Length::Fill),
                    button(text("×")).on_press(Message::DismissSummary),
                ]
                .spacing(6),
            );
            col = col.push(text(self.ai_summary.as_str()));
        }

        if !self.ai_suggested_tags.is_empty() {
            col = col.push(text("AI suggested tags:"));
            col = col.push(
                row![
                    text(self.ai_suggested_tags.join(", ")),
                    button(text("Use")).on_press(Message::UseAiTags),
                    button(text("Replace")).on_press(Message::ReplaceAiTags),
                ]
                .spacing(6),
            );
        }

        col = col
            .push(text_input("tags, comma-separated", &self.tags).on_input(Message::TagsChanged));
        col = col.push(text("Enter text below"));
        col = col.push(
            text_editor(&self.content)
                .on_action(Message::EditContent)
                .height(Length::Fill),
        );
        if !self.status.is_empty() {
            col = col.push(text(self.status.as_str()).size(11));
        }
        col.into()
    }

    fn detail_page<'a>(
        &'a self,
        content: &'a text_editor::Content,
        tags: &'a str,
    ) -> Element<'a, Message> {
        let actions = row![
            button(text("‹ Back")).on_press(Message::CloseDetail),
            button(text("Save changes"))
                .on_press_maybe((!self.detail_busy).then_some(Message::SaveEdit)),
            button(text("Delete"))
                .on_press_maybe((!self.detail_busy).then_some(Message::DeleteNote)),
        ]
        .spacing(6);

        let mut col = column![actions].spacing(10);
        col =
            col.push(text_input("tags, comma-separated", tags).on_input(Message::EditTagsChanged));
        col = col.push(
            text_editor(content)
                .on_action(Message::EditBody)
                .height(Length::Fill),
        );
        col.into()
    }

    fn sync_page(&self) -> Element<'_, Message> {
        // Server side.
        let mut server = column![text("As server").size(14)].spacing(8);
        match &self.server {
            Some(handle) => {
                server = server.push(button(text("Stop Server")).on_press(Message::StopServer));
                server = server.push(text("Pairing code (scan on the other device):"));
                server = server.push(text(handle.pairing_code.as_str()).size(12));
                if let Some(qr) = &self.server_qr {
                    server = server.push(QRCode::new(&qr.0));
                }
            }
            None => {
                server = server.push(button(text("Start Server")).on_press(Message::StartServer));
                server = server.push(
                    text("Serves your notes over TLS; a fresh pairing code is generated each session.")
                        .size(11),
                );
            }
        }

        // Client side.
        let client = column![
            text("As client").size(14),
            text("Paste the pairing code from the other device:").size(11),
            text_input("FASTXT1:192.168.x.x:3456:…:…", &self.pairing_input)
                .on_input(Message::PairingInputChanged)
                .on_submit(Message::Sync),
            button(text("Sync")).on_press_maybe(
                (!self.sync_busy && !self.pairing_input.trim().is_empty()).then_some(Message::Sync)
            ),
            text(self.sync_status.as_str()).size(11),
        ]
        .spacing(8);

        row![
            container(server).width(Length::FillPortion(1)),
            container(client).width(Length::FillPortion(1)),
        ]
        .spacing(20)
        .into()
    }

    fn ai_settings_page(&self) -> Element<'_, Message> {
        let batch_running = self.batch.is_some();
        let col = column![
            text("AI Settings").size(18),
            text("Settings are stored with your notes and shared with the CLI and MCP server.")
                .size(11),
            text("Backend:"),
            pick_list(
                BACKENDS.to_vec(),
                Some(self.backend),
                Message::BackendPicked
            ),
            text("Endpoint (empty = default for the backend):").size(11),
            text_input("http://localhost:11434", &self.endpoint).on_input(Message::EndpointChanged),
            text("Chat model (tagging, summaries, categories):").size(11),
            text_input("llama3.2", &self.model).on_input(Message::ModelChanged),
            text("Embedding model (semantic search):").size(11),
            text_input("nomic-embed-text", &self.embedding_model)
                .on_input(Message::EmbeddingModelChanged),
            text("Timeout (seconds):").size(11),
            text_input("60", &self.timeout).on_input(Message::TimeoutChanged),
            row![
                button(text("Save Settings"))
                    .on_press_maybe((!self.settings_busy).then_some(Message::SaveSettings)),
                button(text("Test Connection"))
                    .on_press_maybe((!self.settings_busy).then_some(Message::TestConnection)),
            ]
            .spacing(6),
            text(self.status.as_str()).size(11),
            text("Batch operations").size(14),
            row![
                button(text("Tag All")).on_press_maybe(
                    (!batch_running).then_some(Message::StartBatch(BatchKind::Tag))
                ),
                button(text("Embed All")).on_press_maybe(
                    (!batch_running).then_some(Message::StartBatch(BatchKind::Embed))
                ),
                button(text("Organize")).on_press_maybe(
                    (!batch_running).then_some(Message::StartBatch(BatchKind::Organize))
                ),
            ]
            .spacing(6),
        ]
        .spacing(8);

        let mut col = col;
        if let Some(state) = &self.batch {
            col = col.push(text(format!(
                "{}… {} / {}",
                state.kind.label(),
                state.done,
                state.total.max(state.done)
            )));
            col = col.push(button(text("Cancel")).on_press(Message::CancelBatch));
        }
        scrollable(col).height(Length::Fill).into()
    }
}

/// Whether the current summary belongs to the current form text.
/// Reset whenever the form is edited? Simplified: tracked by flag.
fn fresh_query(current: &ListQuery) -> ListQuery {
    match current {
        ListQuery::Browse { category, .. } => ListQuery::Browse {
            category: category.clone(),
            offset: 0,
        },
        ListQuery::Search { query, .. } => ListQuery::Search {
            query: query.clone(),
            offset: 0,
        },
        ListQuery::Semantic { query } => ListQuery::Semantic {
            query: query.clone(),
        },
    }
}

/// A search-mode toggle button with a check mark when selected.
fn note_card(note: &NoteCard) -> Element<'_, Message> {
    let mut col = column![].spacing(2);
    if let Some(similarity) = note.similarity {
        col = col.push(text(format!("🔍 {:.0}% similar", similarity * 100.0)).size(11));
    }
    let preview: String = note.txt.chars().take(120).collect();
    col = col.push(text(preview).size(13));
    let mut meta = format!("[{}]", note.tags);
    if !note.ai_tags.is_empty() {
        meta.push_str(&format!(" (AI: {})", note.ai_tags));
    }
    col = col.push(text(meta).size(11));
    if !note.summary.is_empty() {
        col = col.push(text(format!("📝 {}", note.summary)).size(11));
    }
    if let Some(category) = &note.category {
        col = col.push(text(format!("🏷 {category} · {}", note.created_at)).size(11));
    } else {
        col = col.push(text(format!("📅 {}", note.created_at)).size(11));
    }
    button(col)
        .on_press(Message::OpenNote(note.rowid))
        .padding(6)
        .into()
}
