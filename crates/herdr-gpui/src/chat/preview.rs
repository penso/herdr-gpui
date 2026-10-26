//! Opt-in native rich-chat fixture, reached only through --sidebar-test.
//! Synthetic messages go through the real chat reducer, parser, worker jobs,
//! layout, input controls, and painter. No daemon or agent is contacted.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use gpui::{App, AppContext as _, AsyncApp, Context, Window, WindowHandle, px, size};
use herdr_chat_protocol::{Entry, EntryId, EntryKind, Status, Transcript};
use herdr_client::protocol::{
    AgentStatus, CellData, ClientShellPane, CursorState, FrameData, PaneSurfaceFrame,
    PaneSurfacePane, SurfaceRect,
};

use super::{
    Route, Target,
    prepared::{Body, Part},
    visual::Shown,
};
use crate::{HerdrWindow, config::ChatLayoutMode};

mod input_resize;
mod selection;

const PANE: &str = "rich-chat-preview";
const WAIT: Duration = Duration::from_secs(20);
const MATH: &str = r#"## Weighted estimation

Compare prose x + y = 12 with inline math $x + y = 12$: both should read at the same size.

For each event, the weight is $w_i = \frac{p_i}{1-p_i}$ and the yield is $N = \sum_{i=1}^{n} w_i$.

\[
\hat x_{\rm norm}
=
\frac{c_3}{1.25}
-\tanh(p_2-0.2).
\]

\[
\sigma_N = \sqrt{\sum_{i=1}^{n} w_i^2}
\]

| Region | Events | Predicted yield |
|:--|--:|--:|
| Control | 2,000 | $N_C = 134.2$ |
| Validation | 1,250 | $N_V = 82.6$ |

**The estimate uses the same weights throughout.** Inline math follows the text baseline and display equations have their own row.

```python
weights = probability / (1 - probability)
yield_estimate = weights.sum()
uncertainty = (weights ** 2).sum() ** 0.5
```
"#;
const DIAGRAM: &str = "## Analysis flow\n\n```mermaid\nflowchart LR\n A[Events] --> B[Selection]\n B --> C[Weights]\n C --> D[Estimate]\n```\n\nThe diagram is rendered by the same background worker queue as mathematical expressions.\n";
const CODE: &str = "## Code keeps its structure\n\nInline `left\tright` retains separation. The tab-indented block scrolls horizontally.\n\n```rust\nfn main() {\n\tlet α = \"a deliberately long source line that extends beyond the narrow reading column while keeping its original indentation, Unicode, and copied content intact\";\n\tprintln!(\"{α}\");\n\n\t// A blank source line keeps the same line height.\n}\n```\n\nThis indented Markdown image stays literal code:\n\n    ![example](https://example.org/plot.png)\n";
const USER: &str = "Check **four** with $x^2$, `inline code`, and a [reference](https://example.org).\n\n$$y=x+1$$\n\n```python\nprint(2 ** 2)\n```\n\n| Input | Result |\n|:--|--:|\n| 2 | 4 |";
const CONVERSATION: &str = r#"## A clearer conversation

Messages should be comfortable to read. Ordinary paragraphs, **important details**, and links should share the same typography, whether the answer contains an equation or just a few sentences.

### What changed

- **Readable text:** a proportional font with room between paragraphs.
- **Clear structure:** headings, lists, and code have distinct styles.
- **The same conversation:** the reply button opens your existing terminal.

You can choose a layout using the **Bubbles** button in the header. See the [feature guide](https://github.com/penso/herdr-gpui) for the upstream project.

> This is a synthetic preview. No agent is connected and no message will be sent.
"#;

struct Options {
    case: String,
    capture: Option<PathBuf>,
    hold: Duration,
    layout: ChatLayoutMode,
    light: bool,
    max_width: Option<u16>,
    narrow: bool,
    read_only: bool,
}

impl Options {
    fn env() -> Result<Self> {
        let case = std::env::var("HERDR_TEST_RICH_CHAT").unwrap_or_else(|_| "math".into());
        let case = if case == "1" { "math".to_owned() } else { case };
        if !matches!(
            case.as_str(),
            "math"
                | "code"
                | "diagram"
                | "activity"
                | "activity-open"
                | "attachments"
                | "questions"
                | "selection"
                | "user"
                | "widgets"
                | "widgets-end"
                | "conversation"
                | "existing"
                | "input-resize"
                | "all"
        ) {
            bail!(
                "HERDR_TEST_RICH_CHAT expects math, code, diagram, activity, activity-open, attachments, questions, selection, user, widgets, widgets-end, conversation, existing, input-resize, or all"
            );
        }
        let hold = std::env::var("HERDR_TEST_RICH_CHAT_HOLD_MS")
            .ok()
            .map(|value| value.parse::<u64>())
            .transpose()
            .context("invalid preview hold duration")?
            .unwrap_or(2_000)
            .min(60_000);
        let layout = std::env::var("HERDR_TEST_RICH_CHAT_LAYOUT")
            .ok()
            .map(|name| {
                ChatLayoutMode::ALL
                    .iter()
                    .copied()
                    .find(|mode| mode.name() == name)
                    .context("invalid preview layout")
            })
            .transpose()?
            .unwrap_or(ChatLayoutMode::Bubbles);
        let max_width = std::env::var("HERDR_TEST_RICH_CHAT_MAX_WIDTH")
            .ok()
            .map(|value| value.parse::<u16>())
            .transpose()
            .context("invalid preview reading width")?;
        if max_width.is_some_and(|width| width != 0 && !(320..=4000).contains(&width)) {
            bail!("preview reading width expects 0 or 320..=4000");
        }
        Ok(Self {
            case,
            capture: std::env::var_os("HERDR_TEST_RICH_CHAT_CAPTURE").map(PathBuf::from),
            hold: Duration::from_millis(hold),
            layout,
            max_width,
            light: std::env::var("HERDR_TEST_RICH_CHAT_THEME").as_deref() == Ok("light"),
            narrow: std::env::var("HERDR_TEST_RICH_CHAT_SIZE").as_deref() == Ok("narrow"),
            read_only: std::env::var_os("HERDR_TEST_RICH_CHAT_READ_ONLY").is_some(),
        })
    }
}

pub(crate) fn start(handle: WindowHandle<HerdrWindow>, cx: &mut App) {
    cx.spawn(async move |cx| {
        let result = match Options::env() {
            Ok(options) => run(handle, options, cx).await,
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => {
                eprintln!("RICH CHAT native fixture PASS");
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("RICH CHAT native fixture FAIL: {error:#}");
                std::process::exit(1);
            }
        }
    })
    .detach();
}

async fn run(handle: WindowHandle<HerdrWindow>, options: Options, cx: &mut AsyncApp) -> Result<()> {
    handle.update(cx, |view, window, cx| install(view, window, cx, &options))??;
    let timer = cx.background_executor().clone();
    let deadline = Instant::now() + WAIT;
    let mut settled = 0;
    loop {
        timer.timer(Duration::from_millis(100)).await;
        let ready = handle.update(cx, |view, window, cx| -> Result<bool> {
            fit_surface(view);
            view.prepare_chat_visuals(window, cx);
            cx.notify();
            visuals_ready(view)
        })??;
        // Capture only after finished worker results have had real frames.
        settled = if ready { settled + 1 } else { 0 };
        if settled >= 3 {
            break;
        }
        if Instant::now() >= deadline {
            bail!("rich-chat visual jobs did not finish within {WAIT:?}");
        }
    }
    if options.case == "widgets-end" {
        handle.update(cx, |view, _, cx| {
            if let Some(pane) = view.chats.pane(PANE) {
                pane.list.scroll_to_end();
            }
            cx.notify();
        })?;
        timer.timer(Duration::from_millis(200)).await;
    }
    if options.case == "selection" {
        handle.update(cx, |view, _, cx| selection::Probe::ready(view, cx))??;
    }
    if let Some(path) = options.capture {
        #[cfg(feature = "mockup")]
        {
            let pixels = handle.update(cx, |_, window, _| window.render_to_image())??;
            cx.background_spawn(async move { pixels.save(&path) })
                .await
                .context("saving native fixture screenshot")?;
        }
        #[cfg(not(feature = "mockup"))]
        {
            let _ = path;
            bail!(
                "native screenshot capture requires the mockup feature; capture the test window externally"
            );
        }
    }
    handle.update(cx, |view, _, _| {
        if let Some(pane) = view.chats.pane(PANE) {
            let bounds = pane.list.viewport_bounds();
            eprintln!("RICH CHAT GEOMETRY {}", serde_json::json!({
                "max_width": view.config.chat.max_width,
                "viewport": {"x": f32::from(bounds.origin.x), "y": f32::from(bounds.origin.y), "width": f32::from(bounds.size.width), "height": f32::from(bounds.size.height)},
            }));
        }
    })?;
    let size = handle.update(cx, |_, window, _| window.viewport_size())?;
    eprintln!(
        "RICH CHAT READY case={} theme={} width={} height={} hold_ms={}",
        options.case,
        if options.light { "light" } else { "dark" },
        f32::from(size.width),
        f32::from(size.height),
        options.hold.as_millis()
    );
    // The fixture is its own synthetic daemon: acknowledge grid changes while
    // real native zoom keys are exercised, just as the real peer would. A
    // frozen pre-zoom frame would falsely clip the app after font changes.
    let hold_until = Instant::now() + options.hold;
    let mut selection_probe = selection::Probe::default();
    let mut resize_probe = input_resize::Probe::default();
    while Instant::now() < hold_until {
        timer.timer(Duration::from_millis(50)).await;
        handle.update(cx, |view, window, cx| {
            fit_surface(view);
            view.prepare_chat_visuals(window, cx);
            if options.case == "selection" {
                selection_probe.observe(view, window, cx);
            }
            if options.case == "input-resize" {
                resize_probe.observe(view, window);
            }
            cx.notify();
        })?;
    }
    Ok(())
}

fn install(
    view: &mut HerdrWindow,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
    options: &Options,
) -> Result<()> {
    view.config.features.acp_chats = true;
    view.config.chat.layout = options.layout;
    if let Some(width) = options.max_width {
        view.config.chat.max_width = width;
    }
    view.config.sidebar_style.activity_bar = false;
    view.sidebar_width = Some(if options.narrow { 150. } else { 220. });
    view.theme = if options.light {
        view.config.theme_named("Catppuccin Latte", true)?
    } else {
        crate::config::Theme::default()
    };
    view.theme_light = options.light;
    let mut snapshot = crate::sidebar::layout_tests::snapshot(1);
    snapshot.tabs.truncate(1);
    snapshot.tabs[0].label = "Rich chat preview".into();
    snapshot.workspaces[0].label = "Preview".into();
    snapshot.agents.truncate(1);
    snapshot.agents[0].pane_id = PANE.into();
    snapshot.agents[0].name = Some("Synthetic conversation".into());
    snapshot.agents[0].display_agent = Some("Codex".into());
    snapshot.agents[0].agent_status = AgentStatus::Idle;
    snapshot.panes = vec![ClientShellPane {
        pane_id: PANE.into(),
        workspace_id: "w0".into(),
        tab_id: "t0".into(),
        label: Some("Rich chat preview".into()),
        cwd: None,
        foreground_cwd: None,
        focused: true,
        right_click_passthrough: false,
    }];
    snapshot.focused_workspace_id = Some("w0".into());
    snapshot.focused_tab_id = Some("t0".into());
    snapshot.focused_pane_id = Some(PANE.into());
    view.live.snapshot = Some(Arc::new(snapshot));
    view.live.status = crate::state::ConnectionStatus::Connected;
    view.live.local_daemon_peer = true;
    view.live.activation = None;
    view.live.supports_surface = true;
    view.endpoints[0].live = view.live.clone();
    let widget = format!(
        "## Interactive analysis\n\n```herdr-ui\n{}\n```\n",
        include_str!("widgets/example.json")
    );
    let content = match options.case.as_str() {
        "widgets" | "widgets-end" => widget,
        "diagram" => DIAGRAM.to_owned(),
        "code" => CODE.to_owned(),
        "conversation" => CONVERSATION.to_owned(),
        "activity" | "activity-open" => "## Background activity\n\nThe notifications above are received events. Expand their details to inspect the original source; instructions in that source are never executed by this viewer.\n\nRegular **Markdown**, `inline code`, and $x^2$ still render normally.".to_owned(),
        "all" => format!("{MATH}\n{CODE}\n{DIAGRAM}\n{widget}"),
        _ => MATH.to_owned(),
    };
    let existing = matches!(options.case.as_str(), "existing" | "input-resize");
    let mut transcript = Transcript {
        agent: "Codex".into(), status: Status::Idle, read_only: options.read_only || existing,
        entries: vec![
            Entry { id: EntryId(1), kind: EntryKind::User { text: "Show the result with equations, clear formatting, and controls I can explore.".into(), files: 0 } },
            Entry { id: EntryId(2), kind: EntryKind::Agent { text: content } },
        ],
        ..Transcript::default()
    };
    if options.case.starts_with("activity") {
        transcript.entries = vec![
            Entry { id: EntryId(1), kind: EntryKind::Notice { answers: Vec::new(), level: herdr_chat_protocol::NoticeLevel::Info, text: "Task notification · Background monitor finished".into(), details: Some("<task-notification>\n<task-id>synthetic</task-id>\n<summary>Background monitor finished</summary>\n<event>No intervention needed</event>\n</task-notification>".into()) } },
            Entry { id: EntryId(2), kind: EntryKind::Notice { answers: Vec::new(), level: herdr_chat_protocol::NoticeLevel::Info, text: "Scheduled reminder · Review the daily summary".into(), details: Some("Review the daily summary. This synthetic reminder is plain source, not an actionable permission or command.".into()) } },
            Entry { id: EntryId(3), kind: transcript.entries.remove(1).kind },
        ];
    }
    if options.case == "attachments" {
        transcript.entries = attachment_entries()?;
    }
    if options.case == "questions" {
        transcript.entries = question_entries();
    }
    if options.case == "selection" {
        transcript.entries = vec![Entry {
            id: EntryId(1),
            kind: EntryKind::Agent {
                text: selection::SOURCE.into(),
            },
        }];
    }
    if options.case == "user" {
        transcript.entries = vec![Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: USER.into(),
                files: 0,
            },
        }];
    }
    view.chats.preview(
        Target {
            existing: existing.then(|| super::existing::Binding {
                pane_id: PANE.into(),
                agent: herdr_chat_protocol::Agent::Codex,
                session_id: "synthetic-preview-session".into(),
                herdr_session: None,
                pid: 1,
                shell_pid: 1,
                started_ticks: 1,
            }),
            pane_id: PANE.into(),
            endpoint: herdr_chat_protocol::EndpointId::random(),
            route: Route::Local,
        },
        transcript,
    )?;
    if options.case == "selection"
        && let Some(pane) = view.chats.pane_mut(PANE)
    {
        pane.text_focus = Some(cx.focus_handle());
    }
    if options.case == "activity-open"
        && let Some(pane) = view.chats.pane_mut(PANE)
    {
        pane.expanded.insert(EntryId(1));
    }
    if !options.read_only && !existing {
        let font = view.config.ui.clone();
        let theme = view.theme.clone();
        let input = cx.new(|cx| {
            let mut input = crate::search_input::SearchInput::new(cx);
            input.set_frameless(cx);
            input.set_appearance(font, theme, cx);
            input.set_placeholder("Preview only — no agent is connected", cx);
            input
        });
        if let Some(pane) = view.chats.pane_mut(PANE) {
            pane.composer = Some(input);
        }
    }
    window.set_window_title("Herdr · Rich chat preview");
    window.resize(size(
        px(if options.narrow { 650. } else { 1280. }),
        px(860.),
    ));
    fit_surface(view);
    if existing {
        window.focus(&view.focus, cx);
    }
    cx.notify();
    Ok(())
}

fn fit_surface(view: &mut HerdrWindow) {
    let width =
        (f32::from(view.bounds.size.width) / view.cell_width.max(1.)).clamp(1., 400.) as u16;
    let height = (f32::from(view.bounds.size.height) / view.config.terminal.line_height().max(1.))
        .clamp(1., 200.) as u16;
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let cell = CellData {
        symbol: " ".into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    };
    let mut cells = vec![cell; usize::from(width) * usize::from(height)];
    let existing = view
        .chats
        .pane(PANE)
        .is_some_and(|pane| pane.target.existing.is_some());
    let mut cursor = None;
    if existing && height >= 8 {
        // This is the real terminal painter, fed synthetic input/status cells.
        // The fixture never connects to a daemon or submits any prompt.
        let lines = [
            "",
            "> Explain the uncertainty in more detail",
            "",
            "  GPT-6-Astra ultra   ~/analysis   synthetic session",
            "  Enter to send  |  Shift-Enter newline  |  Esc interrupt",
            "  Paste images and files here using the original agent editor",
            "  Preview only: no agent is connected",
            "",
        ];
        for (offset, line) in lines.iter().enumerate() {
            let row = usize::from(height - 8) + offset;
            for (col, character) in line.chars().take(usize::from(width)).enumerate() {
                let cell = &mut cells[row * usize::from(width) + col];
                cell.symbol = character.to_string();
                cell.fg = 0;
            }
        }
        cursor = Some(CursorState {
            x: 40.min(width.saturating_sub(1)),
            y: height - 7,
            visible: true,
            shape: Default::default(),
        });
    }
    view.live.surface = Some(Arc::new(PaneSurfaceFrame {
        boot_id: "layout-test".into(),
        projection_revision: 1,
        surface_revision: 1,
        frame: FrameData {
            width,
            height,
            cells,
            cursor,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![PaneSurfacePane {
            pane_id: PANE.into(),
            content_revision: 1,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: None,
            focused: true,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 0,
            pixel_height: 0,
        }],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    }));
}

fn visuals_ready(view: &HerdrWindow) -> Result<bool> {
    let pane = view.chats.pane(PANE).context("preview pane disappeared")?;
    let mut ids = Vec::new();
    for entry in &pane.session.transcript().entries {
        let Some(body) = pane.prepared.body(entry.id) else {
            continue;
        };
        match body.as_ref() {
            Body::Image(id) => ids.push(*id),
            Body::Message(parts) | Body::Answers { parts, .. } => {
                for part in parts {
                    ids.extend(match part {
                        Part::Diagram(id, _) => vec![*id],
                        Part::Rich(blocks) => blocks
                            .iter()
                            .flat_map(super::rich::Block::visuals)
                            .collect(),
                        _ => Vec::new(),
                    });
                }
            }
            _ => {}
        }
    }
    for id in ids {
        if pane.prepared.visuals.code(id).is_some() {
            continue;
        }
        match pane.prepared.visuals.shown(id) {
            Shown::Pending => return Ok(false),
            Shown::Failed(error) => bail!("preview visual failed: {error}"),
            Shown::Ready(_) => {}
        }
    }
    Ok(true)
}

fn attachment_entries() -> Result<Vec<Entry>> {
    use herdr_chat_protocol::{Author, Image, NoticeLevel};
    // Synthetic 240×128 RGB checkerboard; no file, URL, or private screenshot.
    const CHECKERBOARD: &str = "iVBORw0KGgoAAAANSUhEUgAAAPAAAACACAIAAAC6FXMEAAAB6ElEQVR42u3SMQ0AIAxFwUpDG8IQws6OBFYMdGhzSQW8/F7sc1NvzJV6+vX/FwbVD7RB9QMNhH6ggdAPtH6gDaofaIPqBxoI/UADoR9o/UAbVD/QBtUPNBD6gQZCP9D6gTaofqANqh9oIPQDDYR+oPUDbVD9QAOhH2gg9AMNhH6g9QNtUP1AA6EfaCD0A61fP9D6gTao/gKgPUx/p36g9QNtUP1AA6EfaCD0A60faIPqB9qg+oEGQj/QQOgHWj/QBtUPtEH1Aw2EfqCB0A+0fqANqh9og+oHGgj9QAOhH2j9QBtUP9BA6AcaCP1AA6EfaP1AG1Q/0EDoBxoI/UDr1w+0fqANqr8CaA/T36kfaP1AG1Q/0EDoBxoI/UDrB9qg+oE2qH6ggdAPNBD6gdYPtEH1A21Q/UADoR9oIPQDrR9og+oH2qD6gQZCP9BA6AdaP9AG1Q80EPqBBkI/0EDoB1o/0AbVDzQQ+oEGQj/Q+vUDrR9og+qvANrD9HfqB1o/0AbVDzQQ+oEGQj/Q+oE2qH6gDaofaCD0Aw2EfqD1A21Q/UAbVD/QQOgHGgj9QOsH2qD6gTaofqCB0A80EPqB1g+0QfUDDYR+oIHQDzQQ+oHWD7RB9QMNhH6ggdAPtH79QOsH2qD6C/Q/rkEStgLPXQwAAAAASUVORK5CYII=";
    Ok(vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                level: NoticeLevel::Info,
                text: "Environment context updated".into(),
                details: Some("<environment_context>\n  <cwd>/tmp/herdr-rich-fixture</cwd>\n</environment_context>".into()),
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Notice {
                answers: Vec::new(),
                level: NoticeLevel::Info,
                text: "Image attachment · Image #1".into(),
                details: Some("<image name=[Image #1] path=\"/tmp/herdr-rich-fixture/checkerboard.png\"></image>".into()),
            },
        },
        Entry {
            id: EntryId(3),
            kind: EntryKind::Image {
                author: Author::User,
                image: Image::new("image/png", CHECKERBOARD.into())
                    .context("synthetic attachment image is invalid")?,
            },
        },
        Entry {
            id: EntryId(4),
            kind: EntryKind::User {
                text: "Can you explain this image and link the notes?".into(),
                files: 0,
            },
        },
        Entry {
            id: EntryId(5),
            kind: EntryKind::Agent {
                text: "The attachment shows a blue checkerboard.\n\n[Changes and verification details](/tmp/herdr-rich-fixture/notes.md)\n\nThe document opens only when you click its link.".into(),
            },
        },
    ])
}

fn question_entries() -> Vec<Entry> {
    use herdr_chat_protocol::{NoticeLevel, QuestionAnswer};
    let answers = vec![
        QuestionAnswer {
            question: "Which model should we use for $y=x^2$?".into(),
            answer: "Use the **quadratic** model with $x=2$.".into(),
        },
        QuestionAnswer {
            question: "How should the example print the result?".into(),
            answer: "```python\nprint(2 ** 2)\n```".into(),
        },
    ];
    let records: Vec<_> = answers
        .iter()
        .enumerate()
        .map(|(index, answer)| {
            serde_json::json!({
                "question": answer.question,
                "answer": answer.answer,
                "questionItemId": format!("synthetic-question-{index}"),
            })
        })
        .collect();
    let details = format!(
        "<send_user_message_question_reply>{}</send_user_message_question_reply>",
        serde_json::json!(records)
    );
    vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::Notice {
                answers,
                details: Some(details),
                level: NoticeLevel::Info,
                text: "Questions answered".into(),
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "I will use these choices in the example.".into(),
            },
        },
    ]
}
