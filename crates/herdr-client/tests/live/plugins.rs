//! A minimal generic plugin, linked only into the sandbox's own registry,
//! that records the context Herdr invokes it with. It checks the daemon
//! contract GPUI relies on: a configured plugin action receives exactly the
//! selection `command.invoke` names, fenced by its content revision, and a
//! `file://` link handler claims `pane.link.activate` while an unclaimed file
//! link comes back for the client to open.

use super::*;
use herdr_client::{
    protocol::{PaneSurfaceFrame, PaneSurfacePane},
    scrollback::{SelectionReadParams, TextPoint},
};
use std::path::Path;

/// Writes the manifest of a plugin whose actions record the invocation context
/// and the clicked URL under `out`, and links it into the sandbox. A key binds
/// the capture action, as a user configures a plugin action for GPUI.
fn link_fixture(sandbox: &Sandbox, binary: &Path) {
    let root = sandbox.dir.join("fixture-plugin");
    let out = sandbox.dir.join("out");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&out).unwrap();
    let record = |file: &str, value: &str| {
        // Written aside and renamed, so a reader never sees half a file.
        format!(
            r#"["/bin/sh", "-c", "printf %s \"${value}\" > '{dir}/{file}.part' && mv '{dir}/{file}.part' '{dir}/{file}'"]"#,
            dir = out.display(),
        )
    };
    fs::write(
        root.join("herdr-plugin.toml"),
        format!(
            r#"id = "fixture.context"
name = "Context fixture"
version = "0.1.0"
min_herdr_version = "0.9.0"
platforms = ["linux", "macos"]

[[actions]]
id = "capture"
title = "Capture context"
contexts = ["pane"]
command = {capture}

[[actions]]
id = "open-link"
title = "Record clicked link"
contexts = ["pane"]
command = {link}

[[link_handlers]]
id = "markdown"
title = "Record Markdown links"
pattern = "^file://.*\\.md$"
action = "open-link"
"#,
            capture = record("context.json", "HERDR_PLUGIN_CONTEXT_JSON"),
            link = record("link.txt", "HERDR_PLUGIN_CLICKED_URL"),
        ),
    )
    .unwrap();
    let status = sandbox
        .command(binary, "plugin-link.log")
        .args(["plugin", "link"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success(), "plugin link failed");
    let config = sandbox.dir.join("config.toml");
    let mut text = fs::read_to_string(&config).unwrap();
    text.push_str(
        "\n[[keys.command]]\nkey = \"prefix+a\"\ntype = \"plugin_action\"\ncommand = \"fixture.context.capture\"\n",
    );
    fs::write(config, text).unwrap();
}

fn read_when_written(path: &Path) -> String {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Ok(text) = fs::read_to_string(path) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "{} was never written",
            path.display()
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn enter() -> ClientPaneInputEvent {
    ClientPaneInputEvent::Key {
        code: ClientKeyCode::Enter,
        modifiers: 0,
        kind: ClientKeyKind::Press,
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    }
}

fn row_text(surface: &PaneSurfaceFrame, row: u16) -> String {
    let width = usize::from(surface.frame.width);
    surface.frame.cells[usize::from(row) * width..][..width]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect()
}

/// Where `text` is printed alone on a row with the shell's prompt below it,
/// so no more output is coming: the pane, the frame row, and the column.
fn printed<'a>(
    surface: &'a PaneSurfaceFrame,
    pane_id: &str,
    text: &str,
) -> Option<(&'a PaneSurfacePane, u16, u16)> {
    let pane = surface.panes.iter().find(|pane| pane.pane_id == pane_id)?;
    let rect = pane.inner_rect;
    (rect.y..rect.y + rect.height.saturating_sub(1)).find_map(|row| {
        let line = row_text(surface, row);
        if line.trim() != text || !row_text(surface, row + 1).trim_start().starts_with("LIVE>") {
            return None;
        }
        // The rows are ASCII here, so a byte offset is a column.
        Some((pane, row, u16::try_from(line.find(text)?).ok()?))
    })
}

impl Session {
    /// Types `command` and waits for `text` to settle on its own row.
    fn print(
        &mut self,
        boot: &str,
        pane: &str,
        command: String,
        text: &str,
    ) -> Arc<PaneSurfaceFrame> {
        self.client
            .handle
            .send_input(
                boot,
                pane,
                vec![ClientPaneInputEvent::TextCommit(command), enter()],
            )
            .unwrap();
        let mut found = None;
        self.until(text, |event| match event {
            ClientEvent::Surface(surface) if printed(surface, pane, text).is_some() => {
                found = Some(surface.clone());
                true
            }
            _ => false,
        });
        found.unwrap()
    }

    fn error_code(&mut self, id: String) -> String {
        let mut code = None;
        self.until("API error", |event| match event {
            ClientEvent::Response {
                request_id,
                response,
            } if *request_id == id => {
                code = Some(
                    response["error"]["code"]
                        .as_str()
                        .unwrap_or("none")
                        .to_owned(),
                );
                true
            }
            _ => false,
        });
        code.unwrap()
    }
}

#[test]
#[ignore = "requires explicit HERDR_TEST_BINARY; links a plugin into an isolated sandbox"]
fn plugin_actions_and_file_link_handlers_live() {
    let mut daemon = Daemon::start_with(link_fixture);
    let mut session = Session::open(&daemon);
    let snapshot = session.snapshot.clone().unwrap();
    let boot = snapshot.boot_id.clone();
    let pane = snapshot.focused_pane_id.clone().unwrap();
    let command = snapshot
        .commands
        .iter()
        .find(|command| {
            command.action == herdr_client::protocol::ClientShellCommandAction::PluginAction
        })
        .expect("the configured plugin action is projected")
        .command_id
        .clone();
    let out = daemon.sandbox.dir.join("out");

    // Split the literal so the echoed command line cannot match the output.
    let marker = format!("SELECTED_{}_TEXT", std::process::id());
    let surface = session.print(
        &boot,
        &pane,
        format!("echo SELECTED_{}\"_TEXT\"", std::process::id()),
        &marker,
    );
    let (painted, row, col) = printed(&surface, &pane, &marker).unwrap();
    let rect = painted.inner_rect;
    let top = painted.scroll.map_or(0, |scroll| {
        u32::try_from(scroll.max_offset_from_bottom - scroll.offset_from_bottom).unwrap()
    });
    let start = TextPoint {
        row: top + u32::from(row - rect.y),
        col: col - rect.x,
    };
    let selection = SelectionReadParams {
        pane_id: pane.clone(),
        anchor: start,
        cursor: TextPoint {
            col: start.col + u16::try_from(marker.len()).unwrap() - 1,
            ..start
        },
        content_revision: Some(painted.content_revision),
    };
    let invoke = |selection: &SelectionReadParams| {
        json!({
            "command_id": command,
            "workspace_id": snapshot.focused_workspace_id,
            "tab_id": snapshot.focused_tab_id,
            "pane_id": pane,
            "selection": selection,
        })
    };

    // A revision the pane never showed is refused before the plugin runs.
    let stale = SelectionReadParams {
        content_revision: Some(painted.content_revision + 2),
        ..selection.clone()
    };
    let id = session
        .client
        .handle
        .request(&boot, Method::CommandInvoke, invoke(&stale))
        .unwrap();
    assert_eq!(session.error_code(id), "stale_content");
    assert!(!out.join("context.json").exists());

    let id = session
        .client
        .handle
        .request(&boot, Method::CommandInvoke, invoke(&selection))
        .unwrap();
    session.response(id);
    let context: Value =
        serde_json::from_str(&read_when_written(&out.join("context.json"))).unwrap();
    assert_eq!(context["selected_text"], marker.as_str(), "{context}");
    eprintln!("plugin action received the selected text: {marker}");

    // OSC 8 hyperlinks to two files: the handler claims the Markdown one,
    // and the other comes back unclaimed, for the client to open itself.
    for (name, handled) in [("notes.md", true), ("notes.txt", false)] {
        let url = format!("file://{}/{name}", daemon.sandbox.dir.display());
        let label = format!("link-{}", name.replace('.', "-"));
        let surface = session.print(
            &boot,
            &pane,
            format!(r"printf '\033]8;;{url}\033\\{label}\033]8;;\033\\\n'"),
            &label,
        );
        let (painted, row, col) = printed(&surface, &pane, &label).unwrap();
        let id = session
            .client
            .handle
            .request(
                &boot,
                Method::PaneLinkActivate,
                json!({
                    "pane_id": pane,
                    "viewport_row": row - painted.inner_rect.y,
                    "col": col - painted.inner_rect.x + 1,
                    "content_revision": painted.content_revision,
                    "offset_from_bottom": painted.scroll.map(|scroll| scroll.offset_from_bottom),
                }),
            )
            .unwrap();
        let result = session.response(id);
        assert_eq!(result["type"], "pane_link_activated", "{result}");
        assert_eq!(result["handled"], handled, "{result}");
        assert_eq!(result["url"], url.as_str(), "{result}");
        if handled {
            assert_eq!(read_when_written(&out.join("link.txt")), url);
        }
        eprintln!("file link {name}: handled={handled}");
    }
    assert!(daemon.child.as_mut().unwrap().try_wait().unwrap().is_none());
}
