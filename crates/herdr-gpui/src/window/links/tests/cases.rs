//! Every kind of terminal link, one case each, under each user action: rest,
//! hover, hover with the link modifier held, and a click with it held. The
//! cases follow the table of link behavior in the pull request that added
//! them, in the same order.
//!
//! They run headless: what is drawn is read from the painter's decorations
//! and the window's hover state, and what a click opens from the test
//! platform. Most cases write Unix paths and run only on Unix; the cases
//! whose text is the same on every platform run everywhere.
#![cfg_attr(not(unix), allow(dead_code))]

use super::*;
use crate::{config::FileTarget, menu::Page, terminal_painter::decoration_spans};
use test_case::test_case;

/// A web address too long for one row of the pane, which only the daemon
/// can read whole.
const WRAPPED: &str = concat!(
    "https://example.com/",
    "segment/segment/segment/segment/segment/segment/segment/segment/segment/segment/",
    "segment/segment/segment/segment/segment/segment/segment/segment/segment/segment/",
    "segment/segment/segment/segment/segment/segment/segment/segment/segment/segment/",
    "end",
);
const WIDTH: u16 = 120;
/// The cell the pointer hovers and clicks: on the link in every case.
pub(super) const LINK: (u16, u16) = (2, 0);
/// The cell the pointer rests on: a row no case writes to.
const ELSEWHERE: (u16, u16) = (2, 3);

/// What the pane shows. `{work}` stands for the pane's working directory,
/// `{work_url}` for its `file://` address, and `{tilde}` for the same
/// directory written from `~/`.
enum Text {
    /// An OSC 8 hyperlink to the target, showing the target as its text.
    Osc8(&'static str),
    Plain(&'static str),
    /// Plain text that wraps onto more rows, on a daemon that resolves it.
    Wrapped,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Line {
    None,
    Dashed,
    Solid,
}

/// What the text looks like: its underline, and whether the pointer over it
/// is the hand cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    line: Line,
    hand: bool,
}

const NONE: Look = Look {
    line: Line::None,
    hand: false,
};
const DASHES: Look = Look {
    line: Line::Dashed,
    hand: false,
};
const DASHES_AND_HAND: Look = Look {
    line: Line::Dashed,
    hand: true,
};
const SOLID_AND_HAND: Look = Look {
    line: Line::Solid,
    hand: true,
};

/// What a click with the link modifier held does. Files open in the
/// system's application, which the test platform records.
enum Click {
    Nothing,
    /// Opens the file or folder at this path under the working directory.
    Opens(&'static str),
    /// Opens the file, which names this line for the editor.
    OpensAt(&'static str, u32),
    /// Opens the address at once.
    Address(&'static str),
    /// Asks first, and opens the address once the user says yes.
    Asks(&'static str),
    /// Shows that the file at this path under the working directory is not
    /// found.
    NotFound(&'static str),
    /// Shows this error.
    Says(crate::Error),
}

/// What a click did, as far as the test platform and the window show it.
#[derive(Debug, PartialEq)]
struct Outcome {
    opened: Option<String>,
    message: Option<String>,
    asked: bool,
    line: Option<u32>,
}

/// A working directory for the pane, under the home folder so the cases can
/// name it from `~/`. It holds `src/sample.rs`, `Cargo.toml`, and `crates`,
/// and is removed when dropped.
pub(super) struct Sandbox {
    _root: tempfile::TempDir,
    pub(super) work: std::path::PathBuf,
    work_url: String,
    tilde: String,
}

impl Sandbox {
    pub(super) fn new() -> Self {
        let home = crate::config::home().unwrap();
        let root = tempfile::Builder::new()
            .prefix("herdr-gpui-links-")
            .tempdir_in(&home)
            .unwrap();
        let work = root.path().join("work");
        std::fs::create_dir_all(work.join("src")).unwrap();
        std::fs::create_dir(work.join("crates")).unwrap();
        std::fs::write(
            work.join("src/sample.rs"),
            "// 1\nfn main() {\n    // 3\n}\n",
        )
        .unwrap();
        std::fs::write(work.join("Cargo.toml"), "[workspace]\n").unwrap();
        let tilde = format!("~/{}", work.strip_prefix(&home).unwrap().to_str().unwrap());
        Self {
            _root: root,
            work_url: url::Url::from_file_path(&work).unwrap().into(),
            work,
            tilde,
        }
    }

    pub(super) fn fill(&self, template: &str) -> String {
        template
            .replace("{work_url}", &self.work_url)
            .replace("{work}", self.work.to_str().unwrap())
            .replace("{tilde}", &self.tilde)
    }

    /// The address the system is asked to open for `path`.
    pub(super) fn opened(&self, path: &str) -> String {
        let real = self.work.join(path).canonicalize().unwrap();
        url::Url::from_file_path(real).unwrap().into()
    }

    fn outcome(&self, click: &Click) -> Outcome {
        let outcome = Outcome {
            opened: None,
            message: None,
            asked: false,
            line: None,
        };
        match click {
            Click::Nothing => outcome,
            Click::Opens(path) => Outcome {
                opened: Some(self.opened(path)),
                ..outcome
            },
            Click::OpensAt(path, line) => Outcome {
                opened: Some(self.opened(path)),
                line: Some(*line),
                ..outcome
            },
            Click::Address(address) => Outcome {
                opened: Some(self.fill(address)),
                ..outcome
            },
            Click::Asks(_) => Outcome {
                asked: true,
                ..outcome
            },
            Click::NotFound(path) => Outcome {
                message: Some(
                    crate::Error::LinkedFileMissing {
                        path: path
                            .split('/')
                            .fold(self.work.clone(), |path, part| path.join(part)),
                    }
                    .to_string(),
                ),
                ..outcome
            },
            Click::Says(error) => Outcome {
                message: Some(error.to_string()),
                ..outcome
            },
        }
    }
}

/// A pane showing `text` on its first rows, as an OSC 8 hyperlink to it
/// when `hyperlink`.
pub(super) fn showing(text: &str, hyperlink: bool) -> Arc<PaneSurfaceFrame> {
    let mut surface = wrapped(2);
    let shared = Arc::make_mut(&mut surface);
    let height = 4;
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width: WIDTH,
        height,
    };
    let mut symbols = text.chars();
    shared.frame.width = WIDTH;
    shared.frame.height = height;
    shared.frame.cells = (0..usize::from(WIDTH) * usize::from(height))
        .map(|_| {
            let symbol = symbols.next();
            CellData {
                symbol: symbol.unwrap_or(' ').to_string(),
                fg: 0,
                bg: 0,
                modifier: 0,
                skip: false,
                hyperlink: (hyperlink && symbol.is_some()).then_some(0),
            }
        })
        .collect();
    shared.frame.hyperlinks = if hyperlink {
        vec![text.to_owned()]
    } else {
        vec![]
    };
    let pane = &mut shared.panes[0];
    pane.rect = rect;
    pane.inner_rect = rect;
    pane.pixel_width = u32::from(WIDTH) * 10;
    pane.pixel_height = u32::from(height) * 20;
    surface
}

fn look(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> Look {
    view.read_with(cx, |view, _| {
        let frame = &view.live.surface.as_deref().unwrap().frame;
        let link = usize::from(LINK.1 * frame.width + LINK.0);
        // The text has no underline of its own, so any line the painter
        // draws under it is a hyperlink's dashes.
        let dashed = decoration_spans(frame, link, 8., 20.).next().is_some();
        let solid = view.hovered_local_link().is_some() || view.hovered_daemon_link().is_some();
        Look {
            line: match (solid, dashed) {
                (true, _) => Line::Solid,
                (false, true) => Line::Dashed,
                (false, false) => Line::None,
            },
            hand: view.terminal_link_hovered(position, modifiers),
        }
    })
}

/// Runs `test` with a fresh app, as `gpui::test` does for a function that
/// takes only the app.
fn in_app(test: impl Fn(&mut gpui::TestAppContext)) {
    let test = std::panic::AssertUnwindSafe(test);
    gpui::run_test(
        1,
        &[],
        0,
        &mut |dispatcher, _| {
            let mut cx =
                gpui::TestAppContext::build(dispatcher.clone(), Some("terminal link case"));
            let _entity_refcounts = cx.app.borrow().ref_counts_drop_handle();
            (*test)(&mut cx);
            cx.run_until_parked();
            cx.update(|cx| {
                cx.background_executor().forbid_parking();
                cx.quit();
            });
            cx.run_until_parked();
            drop(cx);
            dispatcher.drain_tasks();
            drop(dispatcher);
        },
        None,
    );
}

/// Answers the daemon's next request about the wrapped address: the cells
/// it covers, or that no handler claimed a click on it.
fn daemon_answers(peer: &mut MockPeer, view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    let request = peer.request();
    let result = match request["method"].as_str() {
        Some("pane.link.resolve") => {
            let last = u16::try_from(WRAPPED.len()).unwrap() - 1;
            let regions = (0..=last / WIDTH)
                .map(|row| {
                    let end = if row == last / WIDTH {
                        last % WIDTH
                    } else {
                        WIDTH - 1
                    };
                    json!({"row": row, "start_col": 0, "end_col": end})
                })
                .collect::<Vec<_>>();
            json!({"type": "pane_link_resolved", "regions": regions})
        }
        Some("pane.link.activate") => {
            json!({"type": "pane_link_activated", "url": WRAPPED, "handled": false})
        }
        method => panic!("unexpected request {method:?}"),
    };
    answer(peer, view, cx, &request, result);
    cx.run_until_parked();
}

/// A window whose pane shows `surface` and works in the sandbox. Files open
/// in the system's application, which the test platform records.
pub(super) fn in_sandbox<'a>(
    sandbox: &Sandbox,
    surface: Arc<PaneSurfaceFrame>,
    peer: &MockPeer,
    daemon: bool,
    cx: &'a mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &'a mut VisualTestContext) {
    let (view, cx) = window(peer, daemon, cx);
    view.update(cx, |view, _| {
        view.live.surface = Some(surface);
        view.config.open_files_in = FileTarget::System;
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        for pane in &mut snapshot.panes {
            pane.cwd = Some(sandbox.work.to_str().unwrap().into());
            pane.foreground_cwd = pane.cwd.clone();
        }
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    (view, cx)
}

fn check(
    text: &Text,
    rest: Look,
    hover: Look,
    cmd_hover: Look,
    cmd_click: &Click,
    cx: &mut gpui::TestAppContext,
) {
    let sandbox = Sandbox::new();
    let daemon = matches!(text, Text::Wrapped);
    let mut peer = if daemon {
        MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"])
    } else {
        MockPeer::new()
    };
    let surface = match text {
        Text::Osc8(target) => showing(&sandbox.fill(target), true),
        Text::Plain(text) => showing(&sandbox.fill(text), false),
        Text::Wrapped => showing(WRAPPED, false),
    };
    let (view, cx) = in_sandbox(&sandbox, surface, &peer, daemon, cx);
    let link = at(&view, cx, LINK.0, LINK.1);
    let elsewhere = at(&view, cx, ELSEWHERE.0, ELSEWHERE.1);
    let (none, held) = (Modifiers::default(), Modifiers::secondary_key());

    cx.simulate_mouse_move(elsewhere, None, none);
    cx.run_until_parked();
    assert_eq!(look(&view, cx, elsewhere, none), rest, "rest");

    cx.simulate_mouse_move(link, None, none);
    cx.run_until_parked();
    assert_eq!(look(&view, cx, link, none), hover, "hover");
    view.read_with(cx, |view, _| {
        assert!(view.links.checked.is_none(), "a hover reads no disk");
        assert!(view.links.hover.is_none(), "a hover asks no daemon");
    });

    cx.simulate_mouse_move(link, None, held);
    cx.run_until_parked();
    if daemon {
        resolve_after_delay(cx);
        daemon_answers(&mut peer, &view, cx);
        view.read_with(cx, |view, _| {
            let rows = view.hovered_daemon_link().unwrap().frame_rows().count();
            assert_eq!(
                rows,
                WRAPPED.len().div_ceil(usize::from(WIDTH)),
                "every row"
            );
        });
    }
    assert_eq!(look(&view, cx, link, held), cmd_hover, "Cmd+hover");

    cx.simulate_click(link, held);
    cx.run_until_parked();
    if daemon {
        daemon_answers(&mut peer, &view, cx);
    }
    let outcome = view.read_with(cx, |view, _| Outcome {
        opened: None,
        message: view.flash.as_ref().map(|(flash, _)| flash.text.to_string()),
        asked: view.menu.page == Some(Page::OpenLink),
        line: view.file_link_at(link).and_then(|file| file.line),
    });
    let outcome = Outcome {
        opened: cx.opened_url(),
        ..outcome
    };
    assert_eq!(outcome, sandbox.outcome(cmd_click), "Cmd+click");
    if let Click::Asks(address) = cmd_click {
        cx.simulate_keystrokes("enter");
        assert_eq!(cx.opened_url(), Some(sandbox.fill(address)), "yes");
    }
}

// OSC 8 hyperlink to a file.
#[cfg_attr(unix, test_case(Text::Osc8("{work_url}/src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "file address, local host, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("file://localhost{work}/src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "file address, localhost, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("{work_url}/src"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src") ; "file address, local host, existing folder"))]
#[test_case(Text::Osc8("{work_url}/src/missing.rs"), DASHES, DASHES, DASHES_AND_HAND, Click::NotFound("src/missing.rs") ; "file address, local host, missing file")]
#[test_case(Text::Osc8("file://other-host/src/sample.rs"), DASHES, DASHES, DASHES_AND_HAND, Click::Says(crate::Error::LinkOnAnotherHost { host: "other-host".into() }) ; "file address, another host")]
#[cfg_attr(unix, test_case(Text::Osc8("{work}/src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "bare absolute path, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("{tilde}/src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "bare home path, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("./src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "bare dot path, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("../work/src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "bare parent path, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "bare relative path, existing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("{work}/src/missing.rs"), DASHES, DASHES, DASHES_AND_HAND, Click::NotFound("src/missing.rs") ; "bare absolute path, missing file"))]
#[cfg_attr(unix, test_case(Text::Osc8("./no/such/file.rs"), DASHES, DASHES, DASHES_AND_HAND, Click::NotFound("./no/such/file.rs") ; "bare dot path, missing file"))]
// OSC 8 hyperlink to a URL.
#[cfg_attr(unix, test_case(Text::Osc8("https://example.com/something/"), DASHES, DASHES, SOLID_AND_HAND, Click::Address("https://example.com/something/") ; "https address"))]
#[cfg_attr(unix, test_case(Text::Osc8("http://example.com/something/"), DASHES, DASHES, SOLID_AND_HAND, Click::Address("http://example.com/something/") ; "http address"))]
#[cfg_attr(unix, test_case(Text::Osc8("mailto:name@example.com"), DASHES, DASHES, SOLID_AND_HAND, Click::Address("mailto:name@example.com") ; "mailto address"))]
#[cfg_attr(unix, test_case(Text::Osc8("vscode://file{work}/src/sample.rs"), DASHES, DASHES, SOLID_AND_HAND, Click::Address("vscode://file{work}/src/sample.rs") ; "vscode address"))]
#[cfg_attr(unix, test_case(Text::Osc8("ssh://example.com"), DASHES, DASHES, SOLID_AND_HAND, Click::Asks("ssh://example.com") ; "another scheme"))]
#[test_case(Text::Osc8("https://"), DASHES, DASHES, DASHES_AND_HAND, Click::Says(crate::Error::MalformedLink) ; "address that is not well formed")]
// Plain text that may be a file path.
#[cfg_attr(unix, test_case(Text::Plain("{work}/src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "absolute path, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("{tilde}/src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "home path, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("./src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "dot path, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("../work/src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "parent path, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "relative path, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("{work}/src/sample.rs:3"), NONE, NONE, SOLID_AND_HAND, Click::OpensAt("src/sample.rs", 3) ; "path with line, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("{work}/src/sample.rs:3:5"), NONE, NONE, SOLID_AND_HAND, Click::OpensAt("src/sample.rs", 3) ; "path with line and column, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("{work}/src"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src") ; "absolute path, existing folder"))]
#[cfg_attr(unix, test_case(Text::Plain("{work}/src/missing.rs"), NONE, NONE, NONE, Click::Nothing ; "absolute path, missing file"))]
#[cfg_attr(unix, test_case(Text::Plain("./no/such/file.rs"), NONE, NONE, NONE, Click::Nothing ; "dot path, missing file"))]
#[cfg_attr(unix, test_case(Text::Plain("Cargo.toml:1"), NONE, NONE, SOLID_AND_HAND, Click::OpensAt("Cargo.toml", 1) ; "bare file name with line, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("Cargo.toml"), NONE, NONE, SOLID_AND_HAND, Click::Opens("Cargo.toml") ; "bare file name with an extension, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("crates"), NONE, NONE, SOLID_AND_HAND, Click::Opens("crates") ; "bare word that names an existing folder"))]
#[cfg_attr(unix, test_case(Text::Plain("no-such-file.txt"), NONE, NONE, NONE, Click::Nothing ; "bare word that names nothing"))]
#[cfg_attr(unix, test_case(Text::Plain("{work_url}/src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Opens("src/sample.rs") ; "file address, existing file"))]
#[cfg_attr(unix, test_case(Text::Plain("{work_url}/src/missing.rs"), NONE, NONE, NONE, Click::Nothing ; "file address, missing file"))]
// Plain text that may be a URL.
#[cfg_attr(unix, test_case(Text::Plain("https://example.com/something/"), NONE, NONE, SOLID_AND_HAND, Click::Address("https://example.com/something/") ; "https address"))]
#[cfg_attr(unix, test_case(Text::Plain("http://example.com/something/"), NONE, NONE, SOLID_AND_HAND, Click::Address("http://example.com/something/") ; "http address"))]
#[cfg_attr(unix, test_case(Text::Wrapped, NONE, NONE, SOLID_AND_HAND, Click::Address(WRAPPED) ; "https address that wraps onto more rows"))]
#[cfg_attr(unix, test_case(Text::Plain("mailto:name@example.com"), NONE, NONE, SOLID_AND_HAND, Click::Address("mailto:name@example.com") ; "mailto address"))]
#[cfg_attr(unix, test_case(Text::Plain("vscode://file{work}/src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Address("vscode://file{work}/src/sample.rs") ; "vscode address"))]
#[cfg_attr(unix, test_case(Text::Plain("vscodium://file{work}/src/sample.rs"), NONE, NONE, SOLID_AND_HAND, Click::Address("vscodium://file{work}/src/sample.rs") ; "vscodium address"))]
#[cfg_attr(unix, test_case(Text::Plain("ssh://example.com"), NONE, NONE, NONE, Click::Nothing ; "another scheme"))]
#[cfg_attr(unix, test_case(Text::Plain("example.com/something/"), NONE, NONE, NONE, Click::Nothing ; "address with no scheme"))]
fn link(text: Text, rest: Look, hover: Look, cmd_hover: Look, cmd_click: Click) {
    in_app(|cx| check(&text, rest, hover, cmd_hover, &cmd_click, cx));
}
