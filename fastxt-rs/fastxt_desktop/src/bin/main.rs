use anyhow::Result;
use druid::widget::{
    Button, CrossAxisAlignment, Either, Flex, Label, LineBreaking, MainAxisAlignment, Scroll,
    SizedBox, Split, TextBox, ViewSwitcher,
};
use druid::{AppLauncher, Color, Data, Insets, Lens, Widget, WidgetExt, WindowDesc};
use fastxt_desktop::utils::qrcode_builder;
use qrcode::{EcLevel, Version};

const SERVER_ADDR: &str = "192.168.3.3:3456";

#[derive(Clone, Data, Lens)]
struct AppState {
    content: String,
    tags: String,
    query: String,
    current_view: RightPanel,
    server_switch: String,
    server_is_on: bool,
    remote_addr: String,
    sync_status: String,
    // AI fields
    ai_suggested_tags: String,
    ai_status: String,
    ai_endpoint: String,
    ai_model: String,
    ai_summary: String,
    // Search mode
    search_mode: SearchMode,
    // Notes list (for display)
    notes_count: String,
    notes_display: String,
    selected_note_id: i64,
    // Category view
    category_view: bool,
    categories_display: String,
}

#[derive(Clone, Data, Copy, PartialEq)]
enum SearchMode {
    Text,
    Semantic,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            content: "".to_string(),
            tags: "".to_string(),
            query: "".to_string(),
            current_view: RightPanel::FormPage,
            server_switch: "Start Server".to_string(),
            server_is_on: false,
            remote_addr: "".to_string(),
            sync_status: "".to_string(),
            ai_suggested_tags: "".to_string(),
            ai_status: "".to_string(),
            ai_endpoint: "http://localhost:11434".to_string(),
            ai_model: "llama3.2".to_string(),
            ai_summary: "".to_string(),
            search_mode: SearchMode::Text,
            notes_count: "0 notes".to_string(),
            notes_display: "Search to display notes".to_string(),
            selected_note_id: 0,
            category_view: false,
            categories_display: "Run 'Organize Notes' to categorize".to_string(),
        }
    }
}

#[derive(Clone, Data, Copy, PartialEq)]
#[allow(clippy::enum_variant_names)]
enum RightPanel {
    SyncPage,
    FormPage,
    AiSettingsPage,
}

fn main() -> Result<()> {
    let main_window = WindowDesc::new(build_root_widget())
        .title("Fastxt")
        .window_size((1000.0, 700.0));

    let initial_state = AppState::default();

    AppLauncher::with_window(main_window)
        .log_to_console()
        .launch(initial_state)?;

    Ok(())
}

fn build_left_panel() -> impl Widget<AppState> {
    Flex::column()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_spacer(10.0)
        .with_child(build_left_header())
        .with_default_spacer()
        .with_child(build_search_mode_toggle())
        .with_default_spacer()
        .with_child(build_left_search())
        .with_default_spacer()
        .with_child(build_category_toggle())
        .with_default_spacer()
        .with_child(Either::new(
            |data: &AppState, _| data.category_view,
            build_category_list(),
            build_listview(),
        ))
        .with_flex_spacer(1.0)
        .padding(Insets::uniform_xy(15.0, 0.0))
}

fn build_left_header() -> impl Widget<AppState> {
    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Baseline)
        .with_child(Label::new("Fastxt").with_text_size(16.0))
        .with_flex_spacer(1.0)
        .with_child(Button::new("AI").on_click(|_, data: &mut AppState, _: &_| {
            data.current_view = RightPanel::AiSettingsPage
        }))
        .with_default_spacer()
        .with_child(
            Button::new("Sync")
                .on_click(|_, data: &mut AppState, _: &_| data.current_view = RightPanel::SyncPage),
        )
        .with_default_spacer()
        .with_child(
            Button::new("Create")
                .on_click(|_, data: &mut AppState, _: &_| data.current_view = RightPanel::FormPage),
        )
}

fn build_search_mode_toggle() -> impl Widget<AppState> {
    Flex::row()
        .with_child(
            Button::new(|data: &AppState, _: &_| {
                if data.search_mode == SearchMode::Text {
                    "Text ✓"
                } else {
                    "Text"
                }
            })
            .on_click(|_, data: &mut AppState, _| {
                data.search_mode = SearchMode::Text;
            }),
        )
        .with_default_spacer()
        .with_child(
            Button::new(|data: &AppState, _: &_| {
                if data.search_mode == SearchMode::Semantic {
                    "Semantic ✓"
                } else {
                    "Semantic"
                }
            })
            .on_click(|_, data: &mut AppState, _| {
                data.search_mode = SearchMode::Semantic;
            }),
        )
}

fn build_category_toggle() -> impl Widget<AppState> {
    Flex::row().with_child(
        Button::new(|data: &AppState, _: &_| {
            if data.category_view {
                "📂 Categories ✓"
            } else {
                "📂 Categories"
            }
        })
        .on_click(|_, data: &mut AppState, _| {
            data.category_view = !data.category_view;
            if data.category_view {
                load_categories(data);
            }
        }),
    )
}

fn build_category_list() -> impl Widget<AppState> {
    Flex::column()
        .with_child(Label::new("Notes by Category").with_text_size(14.0))
        .with_default_spacer()
        .with_flex_child(
            Scroll::new(
                Label::new(|data: &AppState, _: &_| data.categories_display.clone())
                    .with_text_size(12.0)
                    .with_line_break_mode(LineBreaking::WordWrap),
            ),
            1.0,
        )
}

fn build_left_search() -> impl Widget<AppState> {
    Flex::column()
        .with_child(
            Flex::row()
                .with_flex_child(
                    TextBox::new()
                        .with_placeholder("search...")
                        .lens(AppState::query)
                        .expand_width(),
                    1.0,
                )
                .with_default_spacer()
                .with_child(Button::new("Go").on_click(|_, data: &mut AppState, _| {
                    perform_search(data);
                })),
        )
        .with_default_spacer()
        .with_child(Either::new(
            |data: &AppState, _| data.search_mode == SearchMode::Semantic,
            Label::new("Semantic search uses AI to find similar notes")
                .with_text_color(Color::GRAY),
            SizedBox::empty(),
        ))
}

fn build_listview() -> impl Widget<AppState> {
    Flex::column()
        .with_child(Label::new(|data: &AppState, _: &_| {
            data.notes_count.clone()
        }))
        .with_default_spacer()
        .with_child(
            Flex::row()
                .with_child(Label::new("Prev"))
                .with_flex_spacer(1.0)
                .with_child(Label::new("?-?/?"))
                .with_flex_spacer(1.0)
                .with_child(Label::new("Next")),
        )
        .with_default_spacer()
        .with_flex_child(
            Scroll::new(
                Label::new(|data: &AppState, _: &_| data.notes_display.clone())
                    .with_text_size(12.0)
                    .with_line_break_mode(LineBreaking::WordWrap),
            ),
            1.0,
        )
}

fn build_right_panel() -> impl Widget<AppState> {
    ViewSwitcher::new(
        |data: &AppState, _env| data.current_view,
        |selector, _data, _env| match selector {
            RightPanel::FormPage => Box::new(build_form_page()),
            RightPanel::SyncPage => Box::new(build_sync_page()),
            RightPanel::AiSettingsPage => Box::new(build_ai_settings_page()),
        },
    )
}

fn build_form_page() -> impl Widget<AppState> {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .with_spacer(24.0)
        // Action buttons row
        .with_child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Baseline)
                .with_child(Button::new("Clear").on_click(|_, data: &mut AppState, _| {
                    data.content = "".into();
                    data.tags = "".into();
                    data.ai_suggested_tags = "".into();
                    data.ai_status = "".into();
                    data.ai_summary = "".into();
                }))
                .with_default_spacer()
                .with_child(Button::new("Save"))
                .with_default_spacer()
                .with_child(
                    Button::new("AI Tags").on_click(|_, data: &mut AppState, _| {
                        get_ai_tags(data);
                    }),
                )
                .with_default_spacer()
                .with_child(
                    Button::new("Summarize").on_click(|_, data: &mut AppState, _| {
                        get_ai_summary(data);
                    }),
                ),
        )
        .with_default_spacer()
        // AI summary section
        .with_child(Either::new(
            |data: &AppState, _| !data.ai_summary.is_empty(),
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(
                    Flex::row()
                        .with_child(
                            Label::new("Summary:").with_text_color(Color::rgb8(100, 100, 100)),
                        )
                        .with_flex_spacer(1.0)
                        .with_child(Label::new("×").with_text_color(Color::GRAY).on_click(
                            |_, data: &mut AppState, _| {
                                data.ai_summary = "".into();
                            },
                        )),
                )
                .with_child(Label::new(|data: &AppState, _: &_| data.ai_summary.clone()))
                .with_default_spacer(),
            SizedBox::empty(),
        ))
        // AI suggested tags section
        .with_child(Either::new(
            |data: &AppState, _| !data.ai_suggested_tags.is_empty(),
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(Label::new("AI Suggested Tags:"))
                .with_child(
                    Flex::row()
                        .with_child(Label::new(|data: &AppState, _: &_| {
                            data.ai_suggested_tags.clone()
                        }))
                        .with_default_spacer()
                        .with_child(Button::new("Use").on_click(|_, data: &mut AppState, _| {
                            if data.tags.is_empty() {
                                data.tags = data.ai_suggested_tags.clone();
                            } else {
                                data.tags = format!("{},{}", data.tags, data.ai_suggested_tags);
                            }
                            data.ai_suggested_tags = "".into();
                        }))
                        .with_child(Button::new("Replace").on_click(
                            |_, data: &mut AppState, _| {
                                data.tags = data.ai_suggested_tags.clone();
                                data.ai_suggested_tags = "".into();
                            },
                        )),
                )
                .with_default_spacer(),
            SizedBox::empty(),
        ))
        // Tags input
        .with_child(
            TextBox::new()
                .with_placeholder("enter tags: comma or space as tag separator")
                .lens(AppState::tags)
                .expand_width(),
        )
        .with_default_spacer()
        .with_child(Label::new("Enter text below"))
        .with_default_spacer()
        .with_flex_child(TextBox::multiline().lens(AppState::content).expand(), 1.0)
        .with_default_spacer()
        // AI status
        .with_child(Either::new(
            |data: &AppState, _| !data.ai_status.is_empty(),
            Label::new(|data: &AppState, _: &_| data.ai_status.clone()),
            SizedBox::empty(),
        ))
        .padding(15.0)
}

fn build_sync_page() -> impl Widget<AppState> {
    let split = Split::columns(build_sync_left(), build_sync_right())
        .bar_size(1.0)
        .solid_bar(true)
        .draggable(false);
    Flex::row().with_flex_child(split, 1.0).padding(15.0)
}

fn build_sync_left() -> impl Widget<AppState> {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .with_spacer(24.0)
        .with_child(Label::new("As server"))
        .with_default_spacer()
        .with_child(
            Button::new(|data: &AppState, _: &_| data.server_switch.clone()).on_click(
                |_, data: &mut AppState, _: &_| {
                    if data.server_is_on {
                        data.server_is_on = false;
                        data.server_switch = "Start Server".into();
                    } else {
                        data.server_is_on = true;
                        data.server_switch = "Stop Server".into();
                    }
                },
            ),
        )
        .with_child(
            Flex::column()
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .with_child(Either::new(
                    |data: &AppState, _| data.server_is_on,
                    Label::new("Server address:port"),
                    SizedBox::empty(),
                ))
                .with_child(Either::new(
                    |data: &AppState, _| data.server_is_on,
                    Label::new(SERVER_ADDR),
                    SizedBox::empty(),
                )),
        )
        .with_default_spacer()
        .with_child(
            Flex::column()
                .main_axis_alignment(MainAxisAlignment::Center)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Either::new(
                    |data: &AppState, _| data.server_is_on,
                    Label::new("Server QR Code"),
                    SizedBox::empty(),
                ))
                .with_default_spacer()
                .with_child(Either::new(
                    |data: &AppState, _| data.server_is_on,
                    qrcode_builder(
                        SERVER_ADDR,
                        Version::Normal(3),
                        EcLevel::L,
                        (200, 200),
                        "#000000",
                        "#ffffff",
                    )
                    .unwrap(),
                    SizedBox::empty(),
                )),
        )
        .with_flex_spacer(1.0)
        .expand_width()
}

fn build_sync_right() -> impl Widget<AppState> {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .with_spacer(24.0)
        .with_child(Label::new("As Client"))
        .with_default_spacer()
        .with_child(Label::new("Input other Fastxt server's address:port"))
        .with_default_spacer()
        .with_flex_child(
            TextBox::new()
                .with_placeholder("xxx.xxx.xxx.xxx:3456")
                .lens(AppState::remote_addr)
                .expand_width(),
            1.0,
        )
        .with_default_spacer()
        .with_child(Button::new("Sync"))
        .with_default_spacer()
        .with_child(Label::new("Sync status:"))
        .with_default_spacer()
        .with_child(Label::new(|data: &AppState, _: &_| {
            data.sync_status.clone()
        }))
        .expand_width()
        .padding(Insets::uniform_xy(15.0, 0.0))
}

fn build_ai_settings_page() -> impl Widget<AppState> {
    Flex::column()
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .with_spacer(24.0)
        .with_child(Label::new("AI Settings").with_text_size(18.0))
        .with_default_spacer()
        .with_child(Label::new(
            "Configure on-device AI for smart tagging and summarization.",
        ))
        .with_default_spacer()
        .with_child(Label::new("Ollama Endpoint:"))
        .with_child(
            TextBox::new()
                .with_placeholder("http://localhost:11434")
                .lens(AppState::ai_endpoint)
                .expand_width(),
        )
        .with_default_spacer()
        .with_child(Label::new("Model:"))
        .with_child(
            TextBox::new()
                .with_placeholder("llama3.2")
                .lens(AppState::ai_model)
                .expand_width(),
        )
        .with_default_spacer()
        .with_child(
            Button::new("Test Connection").on_click(|_, data: &mut AppState, _| {
                test_ai_connection(data);
            }),
        )
        .with_default_spacer()
        .with_child(Label::new(|data: &AppState, _: &_| data.ai_status.clone()))
        .with_default_spacer()
        .with_child(Label::new("Batch Operations:"))
        .with_default_spacer()
        .with_child(
            Flex::row()
                .with_child(
                    Button::new("Tag All Notes").on_click(|_, data: &mut AppState, _| {
                        batch_tag_all(data);
                    }),
                )
                .with_default_spacer()
                .with_child(Button::new("Embed All Notes").on_click(
                    |_, data: &mut AppState, _| {
                        batch_embed_all(data);
                    },
                )),
        )
        .with_default_spacer()
        .with_child(
            Button::new("Organize Notes").on_click(|_, data: &mut AppState, _| {
                organize_notes(data);
            }),
        )
        .with_child(
            Label::new("Categorize notes into groups (work, personal, etc.)")
                .with_text_color(Color::GRAY),
        )
        .with_default_spacer()
        .with_child(Label::new("Category Management:"))
        .with_default_spacer()
        .with_child(
            Button::new("View Categories").on_click(|_, data: &mut AppState, _| {
                load_categories(data);
            }),
        )
        .with_child(Label::new("Click to see your categorized notes").with_text_color(Color::GRAY))
        .with_default_spacer()
        .with_child(
            Scroll::new(
                Label::new(|data: &AppState, _: &_| data.categories_display.clone())
                    .with_text_size(11.0)
                    .with_line_break_mode(LineBreaking::WordWrap),
            )
            .fix_height(150.0),
        )
        .with_default_spacer()
        .with_child(Label::new("Setup Instructions:"))
        .with_default_spacer()
        .with_child(Label::new("1. Install Ollama: https://ollama.ai"))
        .with_child(Label::new("2. Pull a model: ollama pull llama3.2"))
        .with_child(Label::new(
            "3. Ollama runs automatically as a background service",
        ))
        .with_flex_spacer(1.0)
        .padding(15.0)
}

fn build_root_widget() -> impl Widget<AppState> {
    let split = Split::columns(build_left_panel(), build_right_panel())
        .split_point(1.0 / 4.0)
        .min_size(300.0, 500.0)
        .bar_size(1.0)
        .solid_bar(true)
        .draggable(true);
    Flex::row()
        .with_flex_child(split, 1.0)
        .background(Color::GRAY)
}

/// Perform search based on current search mode.
fn perform_search(data: &mut AppState) {
    if data.query.is_empty() {
        return;
    }

    match data.search_mode {
        SearchMode::Text => {
            let cmd = serde_json::json!({
                "action": "search",
                "query": data.query,
                "limit": 50,
                "offset": 0
            });
            let result = fastxt_core::exe::run(&cmd.to_string());
            // Parse and update notes list
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
                if let Some(count) = response.get("count").and_then(|c| c.as_u64()) {
                    data.notes_count = format!("{} notes found", count);
                }

                // Parse notes array and build display string
                if let Some(notes) = response.get("notes").and_then(|n| n.as_array()) {
                    let mut display_parts = Vec::new();

                    for note in notes {
                        let txt: String = note
                            .get("txt")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .chars()
                            .take(100)
                            .collect();

                        let tags: String = note
                            .get("tags")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();

                        let summary: String = note
                            .get("ai_summary")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();

                        let created_at: String = note
                            .get("created_at")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .split(' ')
                            .next()
                            .unwrap_or("")
                            .to_string();

                        let mut note_display = format!("{}\n[{}]", txt, tags);
                        if !summary.is_empty() {
                            note_display = format!("{}\n📝 Summary: {}", note_display, summary);
                        }
                        note_display = format!("{}\n📅 {}\n---", note_display, created_at);
                        display_parts.push(note_display);
                    }

                    data.notes_display = display_parts.join("\n\n");
                }
            }
        }
        SearchMode::Semantic => {
            data.ai_status = "Searching...".to_string();
            let cmd = serde_json::json!({
                "action": "semantic-search",
                "query": data.query,
                "limit": 20,
                "threshold": 0.5,
                "endpoint": data.ai_endpoint,
                "model": data.ai_model
            });
            let result = fastxt_core::exe::run(&cmd.to_string());
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
                if let Some(results) = response.get("results").and_then(|r| r.as_array()) {
                    data.notes_count = format!("{} similar notes", results.len());
                    data.ai_status = if response
                        .get("available")
                        .and_then(|a| a.as_bool())
                        .unwrap_or(false)
                    {
                        "".to_string()
                    } else {
                        "AI not available for semantic search".to_string()
                    };

                    // Parse semantic search results and build display string
                    let mut display_parts = Vec::new();
                    for r in results {
                        if let Some(note) = r.get("note") {
                            let similarity =
                                r.get("similarity").and_then(|s| s.as_f64()).unwrap_or(0.0);

                            let txt: String = note
                                .get("txt")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .chars()
                                .take(100)
                                .collect();

                            let tags: String = note
                                .get("tags")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .to_string();

                            let summary: String = note
                                .get("ai_summary")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string();

                            let created_at: String = note
                                .get("created_at")
                                .and_then(|t| t.as_str())
                                .unwrap_or("")
                                .split(' ')
                                .next()
                                .unwrap_or("")
                                .to_string();

                            let mut note_display = format!(
                                "🔍 {:.0}% similar\n{}\n[{}]",
                                similarity * 100.0,
                                txt,
                                tags
                            );
                            if !summary.is_empty() {
                                note_display = format!("{}\n📝 Summary: {}", note_display, summary);
                            }
                            note_display = format!("{}\n📅 {}\n---", note_display, created_at);
                            display_parts.push(note_display);
                        }
                    }

                    data.notes_display = display_parts.join("\n\n");
                } else if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
                    data.ai_status = format!("Error: {}", error);
                }
            }
        }
    }
}

/// Get AI-suggested tags for the current content.
fn get_ai_tags(data: &mut AppState) {
    if data.content.is_empty() {
        data.ai_status = "Enter some text first".to_string();
        return;
    }

    data.ai_status = "Getting AI suggestions...".to_string();

    let cmd = serde_json::json!({
        "action": "ai-tag",
        "text": data.content,
        "endpoint": data.ai_endpoint,
        "model": data.ai_model
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(tags) = response.get("tags").and_then(|t| t.as_array()) {
            let tag_str: String = tags
                .iter()
                .filter_map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(",");
            data.ai_suggested_tags = tag_str;
            data.ai_status = if response
                .get("available")
                .and_then(|a| a.as_bool())
                .unwrap_or(false)
            {
                "".to_string()
            } else {
                "AI not available. Check Ollama is running.".to_string()
            };
        } else if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
            data.ai_status = format!("Error: {}", error);
        }
    } else {
        data.ai_status = "Failed to parse AI response".to_string();
    }
}

/// Get AI summary for the current content.
fn get_ai_summary(data: &mut AppState) {
    if data.content.is_empty() {
        data.ai_status = "Enter some text first".to_string();
        return;
    }

    data.ai_status = "Generating summary...".to_string();

    // Save the note first to get a rowid for the summarize command
    let insert_cmd = serde_json::json!({
        "action": "insert",
        "txt": data.content,
        "tags": data.tags,
        "limit": 1,
        "offset": 0
    });
    let insert_result = fastxt_core::exe::run(&insert_cmd.to_string());

    // Get the rowid of the just-inserted note
    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&insert_result) {
        if let Some(notes) = response.get("notes").and_then(|n| n.as_array()) {
            if let Some(note) = notes.first() {
                if let Some(rowid) = note.get("rowid").and_then(|r| r.as_i64()) {
                    let cmd = serde_json::json!({
                        "action": "ai-summarize",
                        "rowid": rowid,
                        "endpoint": data.ai_endpoint,
                        "model": data.ai_model
                    });

                    let result = fastxt_core::exe::run(&cmd.to_string());

                    if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&result) {
                        if let Some(summary) = resp.get("summary").and_then(|s| s.as_str()) {
                            data.ai_summary = summary.to_string();
                            data.ai_status = "".to_string();
                            return;
                        }
                        if let Some(error) = resp.get("error").and_then(|e| e.as_str()) {
                            data.ai_status = format!("Error: {}", error);
                            return;
                        }
                    }
                }
            }
        }
    }

    data.ai_status = "Failed to generate summary".to_string();
}

/// Test AI connection.
fn test_ai_connection(data: &mut AppState) {
    data.ai_status = "Testing connection...".to_string();

    let cmd = serde_json::json!({
        "action": "ai-tag",
        "text": "test",
        "endpoint": data.ai_endpoint,
        "model": data.ai_model
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if response
            .get("available")
            .and_then(|a| a.as_bool())
            .unwrap_or(false)
        {
            data.ai_status = "✓ Connected to Ollama successfully!".to_string();
        } else {
            data.ai_status = "✗ Could not connect to Ollama. Make sure it's running.".to_string();
        }
    } else {
        data.ai_status = "✗ Failed to test connection".to_string();
    }
}

/// Batch tag all notes without AI tags.
fn batch_tag_all(data: &mut AppState) {
    data.ai_status = "Batch tagging notes...".to_string();

    let cmd = serde_json::json!({
        "action": "ai-tag-all",
        "limit": 100,
        "endpoint": data.ai_endpoint,
        "model": data.ai_model
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(processed) = response.get("processed").and_then(|p| p.as_u64()) {
            let errors = response.get("errors").and_then(|e| e.as_u64()).unwrap_or(0);
            data.ai_status = format!("Tagged {} notes ({} errors)", processed, errors);
        } else if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
            data.ai_status = format!("Error: {}", error);
        }
    } else {
        data.ai_status = "Failed to batch tag".to_string();
    }
}

/// Batch embed all notes without embeddings.
fn batch_embed_all(data: &mut AppState) {
    data.ai_status = "Batch embedding notes...".to_string();

    let cmd = serde_json::json!({
        "action": "ai-embed-all",
        "limit": 100,
        "endpoint": data.ai_endpoint,
        "model": data.ai_model
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(processed) = response.get("processed").and_then(|p| p.as_u64()) {
            let errors = response.get("errors").and_then(|e| e.as_u64()).unwrap_or(0);
            data.ai_status = format!("Embedded {} notes ({} errors)", processed, errors);
        } else if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
            data.ai_status = format!("Error: {}", error);
        }
    } else {
        data.ai_status = "Failed to batch embed".to_string();
    }
}

/// Organize notes by AI-generated categories.
fn organize_notes(data: &mut AppState) {
    data.ai_status = "Organizing notes into categories...".to_string();

    let cmd = serde_json::json!({
        "action": "ai-organize",
        "limit": 100,
        "endpoint": data.ai_endpoint,
        "model": data.ai_model
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(processed) = response.get("processed").and_then(|p| p.as_u64()) {
            let errors = response.get("errors").and_then(|e| e.as_u64()).unwrap_or(0);
            let categories = response
                .get("categories")
                .and_then(|c| c.as_object())
                .map(|obj| {
                    obj.iter()
                        .map(|(k, v)| format!("  {}: {} notes", k, v.as_u64().unwrap_or(0)))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            data.ai_status = format!(
                "Organized {} notes ({} errors)\n{}",
                processed, errors, categories
            );
        } else if let Some(error) = response.get("error").and_then(|e| e.as_str()) {
            data.ai_status = format!("Error: {}", error);
        }
    } else {
        data.ai_status = "Failed to organize notes".to_string();
    }
}

/// Load notes grouped by category.
fn load_categories(data: &mut AppState) {
    // Query all notes and group by ai_category
    let cmd = serde_json::json!({
        "action": "select",
        "limit": 1000,
        "offset": 0
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        use std::collections::HashMap;
        let mut categories: HashMap<String, Vec<String>> = HashMap::new();

        if let Some(notes) = response.get("notes").and_then(|n| n.as_array()) {
            for note in notes {
                let category = note
                    .get("ai_category")
                    .and_then(|c| c.as_str())
                    .unwrap_or("uncategorized");

                let txt: String = note
                    .get("txt")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(50)
                    .collect();

                categories
                    .entry(category.to_string())
                    .or_default()
                    .push(txt);
            }
        }

        // Build display string
        let mut display_parts = Vec::new();
        for (category, notes) in categories.iter() {
            display_parts.push(format!("📁 {} ({} notes)", category, notes.len()));
            for note in notes.iter().take(5) {
                display_parts.push(format!("  • {}", note));
            }
            if notes.len() > 5 {
                display_parts.push(format!("  ... and {} more", notes.len() - 5));
            }
            display_parts.push("".to_string());
        }

        data.categories_display = if display_parts.is_empty() {
            "No notes found. Run 'Organize Notes' to categorize.".to_string()
        } else {
            display_parts.join("\n")
        };
    } else {
        data.categories_display = "Failed to load categories".to_string();
    }
}
