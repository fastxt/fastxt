use anyhow::Result;
use druid::widget::{
    Button, CrossAxisAlignment, Either, Flex, Label, MainAxisAlignment, Maybe, SizedBox, Split,
    TextBox, ViewSwitcher,
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
        }
    }
}

#[derive(Clone, Data, Copy, PartialEq)]
enum RightPanel {
    SyncPage,
    FormPage,
    AiSettingsPage,
}

fn main() -> Result<()> {
    let main_window = WindowDesc::new(build_root_widget())
        .title("Fastxt")
        .window_size((900.0, 600.0));

    let initial_state = AppState::default();

    AppLauncher::with_window(main_window)
        .log_to_console()
        .launch(initial_state)?;

    Ok(())
}

fn build_left_panel() -> impl Widget<AppState> {
    Flex::column()
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
        .with_spacer(39.0)
        .with_child(build_left_header())
        .with_default_spacer()
        .with_child(build_left_search())
        .with_default_spacer()
        .with_child(build_listview())
        .with_flex_spacer(1.0)
        .padding(Insets::uniform_xy(15.0, 0.0))
}

fn build_left_header() -> impl Widget<AppState> {
    Flex::row()
        .cross_axis_alignment(CrossAxisAlignment::Baseline)
        .with_child(Label::new("Fastxt"))
        .with_flex_spacer(1.0)
        .with_child(
            Button::new("AI")
                .on_click(|_, data: &mut AppState, _: &_| {
                    data.current_view = RightPanel::AiSettingsPage
                }),
        )
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

fn build_left_search() -> impl Widget<AppState> {
    Flex::row()
        .with_flex_child(
            TextBox::new()
                .with_placeholder("type to search")
                .lens(AppState::query)
                .expand_width(),
            1.0,
        )
        .with_default_spacer()
        .with_child(Button::new("X"))
}

fn build_listview() -> impl Widget<AppState> {
    Flex::row()
        .with_child(Label::new("Prev"))
        .with_flex_spacer(1.0)
        .with_child(Label::new("?-?/?"))
        .with_flex_spacer(1.0)
        .with_child(Label::new("Next").on_click(|_ctx, _state, _env| println!("Next")))
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
        .with_child(
            Flex::row()
                .cross_axis_alignment(CrossAxisAlignment::Baseline)
                .with_child(Button::new("Clear").on_click(|_, data: &mut AppState, _| {
                    data.content = "".into();
                    data.tags = "".into();
                    data.ai_suggested_tags = "".into();
                    data.ai_status = "".into();
                }))
                .with_default_spacer()
                .with_child(Button::new("Save"))
                .with_default_spacer()
                .with_child(
                    Button::new("AI Tags").on_click(|_, data: &mut AppState, _| {
                        get_ai_tags(data);
                    }),
                ),
        )
        .with_default_spacer()
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
                        .with_child(
                            Button::new("Use")
                                .on_click(|_, data: &mut AppState, _| {
                                    if data.tags.is_empty() {
                                        data.tags = data.ai_suggested_tags.clone();
                                    } else {
                                        data.tags = format!("{},{}", data.tags, data.ai_suggested_tags);
                                    }
                                    data.ai_suggested_tags = "".into();
                                }),
                        ),
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
        .with_child(
            Button::new("Test Connection").on_click(|_, data: &mut AppState, _| {
                test_ai_connection(data);
            }),
        )
        .with_default_spacer()
        .with_child(Label::new(|data: &AppState, _: &_| data.ai_status.clone()))
        .with_default_spacer()
        .with_child(Label::new("Setup Instructions:"))
        .with_default_spacer()
        .with_child(Label::new("1. Install Ollama: https://ollama.ai"))
        .with_child(Label::new("2. Pull a model: ollama pull llama3.2"))
        .with_child(Label::new("3. Ollama runs automatically as a background service"))
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
        "endpoint": data.ai_endpoint
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    // Parse the result
    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if let Some(tags) = response.get("tags").and_then(|t| t.as_array()) {
            let tag_str: String = tags
                .iter()
                .filter_map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(",");
            data.ai_suggested_tags = tag_str;
            data.ai_status = if response.get("available").and_then(|a| a.as_bool()).unwrap_or(false) {
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

/// Test AI connection.
fn test_ai_connection(data: &mut AppState) {
    data.ai_status = "Testing connection...".to_string();

    let cmd = serde_json::json!({
        "action": "ai-tag",
        "text": "test",
        "endpoint": data.ai_endpoint
    });

    let result = fastxt_core::exe::run(&cmd.to_string());

    if let Ok(response) = serde_json::from_str::<serde_json::Value>(&result) {
        if response.get("available").and_then(|a| a.as_bool()).unwrap_or(false) {
            data.ai_status = "✓ Connected to Ollama successfully!".to_string();
        } else {
            data.ai_status = "✗ Could not connect to Ollama. Make sure it's running.".to_string();
        }
    } else {
        data.ai_status = "✗ Failed to test connection".to_string();
    }
}
