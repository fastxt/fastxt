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

//! Iced desktop GUI for Fastxt.
//!
//! This is the view layer only: all database / sync / AI logic lives in
//! [`crate::commands`] and is invoked off the UI thread via [`run_blocking`]
//! (a `tokio::task::spawn_blocking` bridge) so that long batch operations never
//! freeze the window. The architecture mirrors the sister project's
//! `localnative_iced` so the two AGPL apps can track Iced versions in lockstep.

use iced::widget::{
    QRCode, Space, button, column, container, qr_code, row, scrollable, text, text_editor,
    text_input,
};
use iced::{Element, Length, Task};

use crate::commands::{self, NoteCard, SearchOutcome, SummaryOutcome, TagSuggestion};

/// Hardcoded advertised server address (matches the original Druid app; real
/// address discovery via the `server-addr` action is left for a follow-up).
const SERVER_ADDR: &str = "192.168.3.3:3456";

/// Which screen the right-hand panel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightPanel {
    Form,
    Sync,
    AiSettings,
}

/// Search backend selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    Text,
    Semantic,
}

/// Wrapper making `qr_code::Data` `Send`.
///
/// The QR data is only ever created and read on the GUI thread; this wrapper
/// only exists so the application state satisfies Iced's `Send` bounds. (Same
/// pattern as `localnative_iced`.)
struct SendableQrData(qr_code::Data);

// SAFETY: `qr_code::Data` is created and used exclusively on the GUI thread and
// never actually moved across threads.
unsafe impl Send for SendableQrData {}

/// Top-level application state.
struct Fastxt {
    // Form inputs.
    content: text_editor::Content,
    tags: String,
    query: String,

    // Navigation.
    current_view: RightPanel,
    search_mode: SearchMode,
    category_view: bool,

    // Sync / server.
    server_is_on: bool,
    remote_addr: String,
    sync_status: String,
    qrcode: Option<SendableQrData>,

    // AI settings.
    ai_endpoint: String,
    ai_model: String,

    // Transient results / status.
    ai_suggested_tags: String,
    ai_summary: String,
    ai_status: String,
    notes_count: String,
    notes: Vec<NoteCard>,
    categories_display: String,

    /// True while a background command is running; disables action buttons.
    busy: bool,
}

#[derive(Debug, Clone)]
enum Message {
    // Navigation.
    ShowForm,
    ShowSync,
    ShowAiSettings,
    SetSearchMode(SearchMode),
    ToggleCategoryView,

    // Text inputs.
    EditContent(text_editor::Action),
    TagsChanged(String),
    QueryChanged(String),
    RemoteAddrChanged(String),
    EndpointChanged(String),
    ModelChanged(String),

    // Form actions.
    ClearForm,
    Save,
    Saved(Option<i64>),
    RequestAiTags,
    AiTagsReady(TagSuggestion),
    RequestSummary,
    SummaryReady(SummaryOutcome),
    UseAiTags,
    ReplaceAiTags,
    DismissSummary,

    // Search.
    Search,
    SearchReady(SearchOutcome),

    // Sync.
    ToggleServer,
    Sync,
    SyncReady(String),

    // AI settings / batch.
    TestConnection,
    TestConnectionReady(String),
    TagAll,
    EmbedAll,
    Organize,
    BatchReady(String),
    LoadCategories,
    CategoriesReady(String),
}

/// Run a blocking command off the UI thread and map its result into a `Message`.
fn run_blocking<T>(f: impl FnOnce() -> T + Send + 'static, msg: fn(T) -> Message) -> Task<Message>
where
    T: Send + 'static,
{
    Task::perform(
        async move {
            tokio::task::spawn_blocking(f)
                .await
                .expect("blocking command panicked")
        },
        msg,
    )
}

/// Launch the Fastxt desktop application.
///
/// # Errors
/// Returns an error if the Iced runtime fails to start the window.
pub fn run() -> iced::Result {
    iced::application(Fastxt::new, Fastxt::update, Fastxt::view)
        .title("Fastxt")
        .window(iced::window::Settings {
            size: iced::Size::new(1000.0, 700.0),
            ..Default::default()
        })
        .run()
}

impl Fastxt {
    fn new() -> (Self, Task<Message>) {
        let state = Self {
            content: text_editor::Content::new(),
            tags: String::new(),
            query: String::new(),
            current_view: RightPanel::Form,
            search_mode: SearchMode::Text,
            category_view: false,
            server_is_on: false,
            remote_addr: String::new(),
            sync_status: String::new(),
            qrcode: None,
            ai_endpoint: "http://localhost:11434".to_string(),
            ai_model: "llama3.2".to_string(),
            ai_suggested_tags: String::new(),
            ai_summary: String::new(),
            ai_status: String::new(),
            notes_count: "0 notes".to_string(),
            notes: Vec::new(),
            categories_display: "Run 'Organize Notes' to categorize".to_string(),
            busy: false,
        };
        // Populate the list with recent notes on startup.
        let boot = run_blocking(|| commands::recent_notes(50, 0), Message::SearchReady);
        (state, boot)
    }

    #[allow(clippy::too_many_lines)]
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ---- Navigation ----
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
            Message::SetSearchMode(mode) => {
                self.search_mode = mode;
                Task::none()
            }
            Message::ToggleCategoryView => {
                self.category_view = !self.category_view;
                if self.category_view {
                    self.busy = true;
                    run_blocking(commands::load_categories, Message::CategoriesReady)
                } else {
                    Task::none()
                }
            }

            // ---- Text inputs ----
            Message::EditContent(action) => {
                self.content.perform(action);
                Task::none()
            }
            Message::TagsChanged(s) => {
                self.tags = s;
                Task::none()
            }
            Message::QueryChanged(s) => {
                self.query = s;
                Task::none()
            }
            Message::RemoteAddrChanged(s) => {
                self.remote_addr = s;
                Task::none()
            }
            Message::EndpointChanged(s) => {
                self.ai_endpoint = s;
                Task::none()
            }
            Message::ModelChanged(s) => {
                self.ai_model = s;
                Task::none()
            }

            // ---- Form actions ----
            Message::ClearForm => {
                self.content = text_editor::Content::new();
                self.tags.clear();
                self.ai_suggested_tags.clear();
                self.ai_status.clear();
                self.ai_summary.clear();
                Task::none()
            }
            Message::Save => {
                let content = self.content.text();
                if content.trim().is_empty() {
                    self.ai_status = "Enter some text first".to_string();
                    return Task::none();
                }
                let tags = self.tags.clone();
                self.busy = true;
                run_blocking(
                    move || commands::insert_note(&content, &tags),
                    Message::Saved,
                )
            }
            Message::Saved(rowid) => {
                self.busy = false;
                if rowid.is_some() {
                    self.content = text_editor::Content::new();
                    self.tags.clear();
                    self.ai_suggested_tags.clear();
                    self.ai_summary.clear();
                    self.ai_status = "Saved".to_string();
                } else {
                    self.ai_status = "Failed to save".to_string();
                }
                Task::none()
            }
            Message::RequestAiTags => {
                let content = self.content.text();
                if content.trim().is_empty() {
                    self.ai_status = "Enter some text first".to_string();
                    return Task::none();
                }
                let endpoint = self.ai_endpoint.clone();
                let model = self.ai_model.clone();
                self.busy = true;
                self.ai_status = "Getting AI suggestions...".to_string();
                run_blocking(
                    move || commands::ai_tags(&content, &endpoint, &model),
                    Message::AiTagsReady,
                )
            }
            Message::AiTagsReady(suggestion) => {
                self.busy = false;
                self.ai_suggested_tags = suggestion.tags;
                self.ai_status = suggestion.status;
                Task::none()
            }
            Message::RequestSummary => {
                let content = self.content.text();
                if content.trim().is_empty() {
                    self.ai_status = "Enter some text first".to_string();
                    return Task::none();
                }
                let tags = self.tags.clone();
                let endpoint = self.ai_endpoint.clone();
                let model = self.ai_model.clone();
                self.busy = true;
                self.ai_status = "Generating summary...".to_string();
                run_blocking(
                    move || commands::summarize_new_note(&content, &tags, &endpoint, &model),
                    Message::SummaryReady,
                )
            }
            Message::SummaryReady(outcome) => {
                self.busy = false;
                self.ai_summary = outcome.summary;
                self.ai_status = outcome.status;
                Task::none()
            }
            Message::UseAiTags => {
                if self.tags.is_empty() {
                    self.tags = self.ai_suggested_tags.clone();
                } else {
                    self.tags = format!("{},{}", self.tags, self.ai_suggested_tags);
                }
                self.ai_suggested_tags.clear();
                Task::none()
            }
            Message::ReplaceAiTags => {
                self.tags = std::mem::take(&mut self.ai_suggested_tags);
                Task::none()
            }
            Message::DismissSummary => {
                self.ai_summary.clear();
                Task::none()
            }

            // ---- Search ----
            Message::Search => {
                if self.query.is_empty() {
                    return Task::none();
                }
                self.busy = true;
                match self.search_mode {
                    SearchMode::Text => {
                        let query = self.query.clone();
                        run_blocking(
                            move || commands::text_search(&query, 50, 0),
                            Message::SearchReady,
                        )
                    }
                    SearchMode::Semantic => {
                        self.ai_status = "Searching...".to_string();
                        let query = self.query.clone();
                        let endpoint = self.ai_endpoint.clone();
                        let model = self.ai_model.clone();
                        run_blocking(
                            move || commands::semantic_search(&query, &endpoint, &model),
                            Message::SearchReady,
                        )
                    }
                }
            }
            Message::SearchReady(outcome) => {
                self.busy = false;
                self.notes_count = outcome.count_label;
                self.notes = outcome.notes;
                // Assign unconditionally: `status` is empty on success, which must
                // clear the transient "Searching..." set when a semantic search
                // was dispatched (otherwise it lingers after the results arrive).
                self.ai_status = outcome.status;
                Task::none()
            }

            // ---- Sync ----
            Message::ToggleServer => {
                self.server_is_on = !self.server_is_on;
                self.qrcode = if self.server_is_on {
                    qr_code::Data::new(SERVER_ADDR.as_bytes())
                        .ok()
                        .map(SendableQrData)
                } else {
                    None
                };
                Task::none()
            }
            Message::Sync => {
                if self.remote_addr.is_empty() {
                    return Task::none();
                }
                let addr = self.remote_addr.clone();
                self.busy = true;
                self.sync_status = "Syncing...".to_string();
                run_blocking(move || commands::client_sync(&addr), Message::SyncReady)
            }
            Message::SyncReady(status) => {
                self.busy = false;
                self.sync_status = status;
                Task::none()
            }

            // ---- AI settings / batch ----
            Message::TestConnection => {
                let endpoint = self.ai_endpoint.clone();
                let model = self.ai_model.clone();
                self.busy = true;
                self.ai_status = "Testing connection...".to_string();
                run_blocking(
                    move || commands::test_connection(&endpoint, &model),
                    Message::TestConnectionReady,
                )
            }
            Message::TestConnectionReady(status) => {
                self.busy = false;
                self.ai_status = status;
                Task::none()
            }
            Message::TagAll => {
                let endpoint = self.ai_endpoint.clone();
                let model = self.ai_model.clone();
                self.busy = true;
                self.ai_status = "Batch tagging notes...".to_string();
                run_blocking(
                    move || commands::tag_all(&endpoint, &model),
                    Message::BatchReady,
                )
            }
            Message::EmbedAll => {
                let endpoint = self.ai_endpoint.clone();
                let model = self.ai_model.clone();
                self.busy = true;
                self.ai_status = "Batch embedding notes...".to_string();
                run_blocking(
                    move || commands::embed_all(&endpoint, &model),
                    Message::BatchReady,
                )
            }
            Message::Organize => {
                let endpoint = self.ai_endpoint.clone();
                let model = self.ai_model.clone();
                self.busy = true;
                self.ai_status = "Organizing notes into categories...".to_string();
                run_blocking(
                    move || commands::organize(&endpoint, &model),
                    Message::BatchReady,
                )
            }
            Message::BatchReady(status) => {
                self.busy = false;
                self.ai_status = status;
                Task::none()
            }
            Message::LoadCategories => {
                self.busy = true;
                run_blocking(commands::load_categories, Message::CategoriesReady)
            }
            Message::CategoriesReady(display) => {
                self.busy = false;
                self.categories_display = display;
                Task::none()
            }
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let left = container(self.left_panel())
            .width(Length::FillPortion(1))
            .padding(10);
        let right = container(self.right_panel())
            .width(Length::FillPortion(3))
            .padding(10);
        row![left, right].into()
    }

    // ---- Left panel ----

    fn left_panel(&self) -> Element<'_, Message> {
        let header = row![
            text("Fastxt").size(18),
            Space::new().width(Length::Fill),
            button(text("AI")).on_press(Message::ShowAiSettings),
            button(text("Sync")).on_press(Message::ShowSync),
            button(text("Create")).on_press(Message::ShowForm),
        ]
        .spacing(6);

        let mode_toggle = row![
            mode_button(
                "Text",
                self.search_mode == SearchMode::Text,
                SearchMode::Text
            ),
            mode_button(
                "Semantic",
                self.search_mode == SearchMode::Semantic,
                SearchMode::Semantic,
            ),
        ]
        .spacing(6);

        let search_row = row![
            text_input("search...", &self.query)
                .on_input(Message::QueryChanged)
                .on_submit(Message::Search),
            button(text("Go")).on_press_maybe((!self.busy).then_some(Message::Search)),
        ]
        .spacing(6);

        // Gated on `!busy` like the action buttons: toggling on dispatches a
        // background load that clears `busy`, so allowing it mid-operation would
        // re-enable the other buttons while that earlier operation is still running.
        let category_toggle = button(text(if self.category_view {
            "📂 Categories ✓"
        } else {
            "📂 Categories"
        }))
        .on_press_maybe((!self.busy).then_some(Message::ToggleCategoryView));

        let body = if self.category_view {
            self.category_list()
        } else {
            self.notes_list()
        };

        let mut col = column![header, mode_toggle, search_row].spacing(10);
        if self.search_mode == SearchMode::Semantic {
            col = col.push(text("Semantic search uses AI to find similar notes").size(11));
        }
        col = col.push(category_toggle);
        col = col.push(body);
        col.into()
    }

    fn notes_list(&self) -> Element<'_, Message> {
        let mut col = column![text(self.notes_count.as_str())].spacing(8);
        if self.notes.is_empty() {
            col = col.push(text("Search to display notes").size(12));
        }
        for note in &self.notes {
            col = col.push(note_card(note));
        }
        scrollable(col).height(Length::Fill).into()
    }

    fn category_list(&self) -> Element<'_, Message> {
        column![
            text("Notes by Category").size(14),
            scrollable(text(self.categories_display.as_str()).size(12)).height(Length::Fill),
        ]
        .spacing(8)
        .into()
    }

    // ---- Right panel ----

    fn right_panel(&self) -> Element<'_, Message> {
        match self.current_view {
            RightPanel::Form => self.form_page(),
            RightPanel::Sync => self.sync_page(),
            RightPanel::AiSettings => self.ai_settings_page(),
        }
    }

    fn form_page(&self) -> Element<'_, Message> {
        let actions = row![
            button(text("Clear")).on_press(Message::ClearForm),
            button(text("Save")).on_press_maybe((!self.busy).then_some(Message::Save)),
            button(text("AI Tags")).on_press_maybe((!self.busy).then_some(Message::RequestAiTags)),
            button(text("Summarize"))
                .on_press_maybe((!self.busy).then_some(Message::RequestSummary)),
        ]
        .spacing(6);

        let mut col = column![actions].spacing(10);

        if !self.ai_summary.is_empty() {
            col = col.push(
                row![
                    text("Summary:"),
                    Space::new().width(Length::Fill),
                    button(text("×")).on_press(Message::DismissSummary),
                ]
                .spacing(6),
            );
            col = col.push(text(self.ai_summary.as_str()));
        }

        if !self.ai_suggested_tags.is_empty() {
            col = col.push(text("AI Suggested Tags:"));
            col = col.push(
                row![
                    text(self.ai_suggested_tags.as_str()),
                    button(text("Use")).on_press(Message::UseAiTags),
                    button(text("Replace")).on_press(Message::ReplaceAiTags),
                ]
                .spacing(6),
            );
        }

        col = col.push(
            text_input("enter tags: comma or space as tag separator", &self.tags)
                .on_input(Message::TagsChanged),
        );
        col = col.push(text("Enter text below"));
        col = col.push(
            text_editor(&self.content)
                .on_action(Message::EditContent)
                .height(Length::Fill),
        );
        if !self.ai_status.is_empty() {
            col = col.push(text(self.ai_status.as_str()));
        }

        col.into()
    }

    fn sync_page(&self) -> Element<'_, Message> {
        // Server side.
        let mut server = column![
            text("As server"),
            button(text(if self.server_is_on {
                "Stop Server"
            } else {
                "Start Server"
            }))
            .on_press(Message::ToggleServer),
        ]
        .spacing(10);
        if self.server_is_on {
            server = server.push(text("Server address:port"));
            server = server.push(text(SERVER_ADDR));
            if let Some(qr) = &self.qrcode {
                server = server.push(text("Server QR Code"));
                server = server.push(QRCode::new(&qr.0));
            }
        }

        // Client side.
        let client = column![
            text("As Client"),
            text("Input other Fastxt server's address:port"),
            text_input("xxx.xxx.xxx.xxx:3456", &self.remote_addr)
                .on_input(Message::RemoteAddrChanged),
            button(text("Sync")).on_press_maybe(
                (!self.busy && !self.remote_addr.is_empty()).then_some(Message::Sync)
            ),
            text("Sync status:"),
            text(self.sync_status.as_str()),
        ]
        .spacing(10);

        row![
            container(server).width(Length::FillPortion(1)),
            container(client).width(Length::FillPortion(1)),
        ]
        .spacing(20)
        .into()
    }

    fn ai_settings_page(&self) -> Element<'_, Message> {
        let col = column![
            text("AI Settings").size(18),
            text("Configure on-device AI for smart tagging and summarization."),
            text("Ollama Endpoint:"),
            text_input("http://localhost:11434", &self.ai_endpoint)
                .on_input(Message::EndpointChanged),
            text("Model:"),
            text_input("llama3.2", &self.ai_model).on_input(Message::ModelChanged),
            button(text("Test Connection"))
                .on_press_maybe((!self.busy).then_some(Message::TestConnection)),
            text(self.ai_status.as_str()),
            text("Batch Operations:"),
            row![
                button(text("Tag All Notes"))
                    .on_press_maybe((!self.busy).then_some(Message::TagAll)),
                button(text("Embed All Notes"))
                    .on_press_maybe((!self.busy).then_some(Message::EmbedAll)),
            ]
            .spacing(6),
            button(text("Organize Notes"))
                .on_press_maybe((!self.busy).then_some(Message::Organize)),
            text("Categorize notes into groups (work, personal, etc.)").size(12),
            text("Category Management:"),
            button(text("View Categories"))
                .on_press_maybe((!self.busy).then_some(Message::LoadCategories)),
            scrollable(text(self.categories_display.as_str()).size(11))
                .height(Length::Fixed(150.0)),
            text("Setup Instructions:"),
            text("1. Install Ollama: https://ollama.ai"),
            text("2. Pull a model: ollama pull llama3.2"),
            text("3. Ollama runs automatically as a background service"),
        ]
        .spacing(8);

        scrollable(col).height(Length::Fill).into()
    }
}

/// A search-mode toggle button that shows a check mark when selected.
fn mode_button(label: &str, selected: bool, mode: SearchMode) -> button::Button<'_, Message> {
    let caption = if selected {
        format!("{label} ✓")
    } else {
        label.to_string()
    };
    button(text(caption)).on_press(Message::SetSearchMode(mode))
}

/// Render a single note as a compact card.
fn note_card(note: &NoteCard) -> Element<'_, Message> {
    let mut col = column![].spacing(2);
    if let Some(similarity) = note.similarity {
        col = col.push(text(format!("🔍 {:.0}% similar", similarity * 100.0)).size(12));
    }
    let preview: String = note.txt.chars().take(100).collect();
    col = col.push(text(preview).size(13));
    col = col.push(text(format!("[{}]", note.tags)).size(11));
    if !note.summary.is_empty() {
        col = col.push(text(format!("📝 {}", note.summary)).size(11));
    }
    if !note.created_at.is_empty() {
        col = col.push(text(format!("📅 {}", note.created_at)).size(11));
    }
    col.into()
}
