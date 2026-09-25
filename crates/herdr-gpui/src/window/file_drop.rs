//! External paths paste locally; one supported image on SSH uses the daemon's
//! image bridge. A drop never submits the terminal command.

use super::HerdrWindow;
use crate::{
    Error, Result,
    connection::ConnectionBridge,
    terminal::{InputTarget, wheel_target},
};
use gpui_kit::{Context, ExternalPaths, Pixels, Point, Window};
use herdr_client::protocol::ClientPaneInputEvent;
use std::path::PathBuf;

const MAX_PATHS: usize = 256;
const MAX_PASTE_BYTES: usize = 64 * 1024;

impl HerdrWindow {
    pub(crate) fn drop_terminal_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // GPUI translates the platform Submit position into window mouse state
        // before invoking on_drop; the payload itself carries no coordinates.
        let Some(target) = self.file_drop_target(window.mouse_position()) else {
            return;
        };
        cx.stop_propagation();
        if paths.paths().is_empty() {
            return;
        }
        let result = quote_paths(paths.paths()).and_then(|text| {
            if self.accepts_remote_images()
                && let [path] = paths.paths()
                && let Some(source) = super::image_source::from_path(path)
            {
                self.start_remote_image(target.clone(), source, Some(text), cx);
                return Ok(());
            }
            if self.accepts_remote_images() {
                self.start_file_transfer(target.clone(), paths.paths().to_vec(), cx);
                return Ok(());
            }
            let endpoint = &self.endpoints[self.selected_endpoint];
            let handle = endpoint
                .connection
                .handle
                .as_ref()
                .ok_or(Error::NotConnected)?;
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            ConnectionBridge::send_input(
                handle,
                &snapshot.boot_id,
                &target,
                ClientPaneInputEvent::Paste(text),
            )?;
            Ok(())
        });
        match result {
            Ok(()) => window.focus(&self.focus, cx),
            Err(error) => {
                self.local_error = Some(format!("Files not pasted: {error}"));
                cx.notify();
            }
        }
    }

    fn file_drop_target(&self, position: Point<Pixels>) -> Option<InputTarget> {
        if self.menu.page.is_some()
            || !self.live.status.is_connected()
            || !self.input_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        wheel_target(
            self.live.surface.as_deref()?,
            f32::from(position.x - self.bounds.origin.x),
            f32::from(position.y - self.bounds.origin.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )
        .map(|hit| hit.target)
    }
}

/// POSIX shell words, separated by spaces without a trailing newline. Validate
/// the whole drop before allocating its output so failures never paste a prefix.
pub(super) fn quote_paths(paths: &[PathBuf]) -> Result<String> {
    if paths.len() > MAX_PATHS {
        return Err(Error::FileDropSize);
    }
    let mut bytes = paths.len().saturating_sub(1);
    for path in paths {
        // Bound UTF-8 validation and quote counting as well as output allocation.
        if path.as_os_str().len() > MAX_PASTE_BYTES {
            return Err(Error::FileDropSize);
        }
        let text = path.to_str().ok_or(Error::FileDropEncoding)?;
        if text.is_empty() {
            return Err(Error::FileDropEmptyPath);
        }
        if text.chars().any(char::is_control) {
            return Err(Error::FileDropControl);
        }
        bytes += text.len() + 2 + text.bytes().filter(|byte| *byte == b'\'').count() * 3;
        if bytes > MAX_PASTE_BYTES {
            return Err(Error::FileDropSize);
        }
    }
    let mut quoted = String::with_capacity(bytes);
    for path in paths {
        if !quoted.is_empty() {
            quoted.push(' ');
        }
        quoted.push('\'');
        for ch in path.to_str().ok_or(Error::FileDropEncoding)?.chars() {
            if ch == '\'' {
                quoted.push_str("'\\''");
            } else {
                quoted.push(ch);
            }
        }
        quoted.push('\'');
    }
    Ok(quoted)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::{sidebar::layout_tests::fixture_window, state::ConnectionStatus};
    use gpui_kit::{AppContext, Bounds, point, prelude::*, px, size};
    use herdr_client::protocol::{
        ClientShellPopupSurface, FrameData, PaneSurfaceFrame, PaneSurfacePane, SurfaceRect,
    };
    use std::sync::Arc;

    #[test]
    fn quotes_shell_metacharacters_apostrophes_and_unicode_without_execution() {
        let paths = [
            "/tmp/a b",
            "/tmp/it's",
            "/tmp/$(touch bad);`echo bad`&|<>*?[]{}!~$HOME\\\"",
            "/tmp/\u{65e5}\u{672c}\u{8a9e}",
        ]
        .map(PathBuf::from);
        assert_eq!(
            quote_paths(&paths).unwrap(),
            "'/tmp/a b' '/tmp/it'\\''s' '/tmp/$(touch bad);`echo bad`&|<>*?[]{}!~$HOME\\\"' '/tmp/\u{65e5}\u{672c}\u{8a9e}'"
        );
    }

    #[test]
    fn rejects_control_characters_atomically_and_redacts_paths() {
        for control in [
            '\0', '\n', '\r', '\t', '\u{1b}', '\u{7f}', '\u{85}', '\u{9b}',
        ] {
            let error = quote_paths(&[
                PathBuf::from("/valid"),
                PathBuf::from(format!("/private{control}path")),
            ])
            .unwrap_err();
            assert!(matches!(error, Error::FileDropControl));
            assert!(!format!("{error} {error:?}").contains("private"));
        }
    }

    #[test]
    fn bounds_path_count_and_expanded_utf8_bytes() {
        assert!(quote_paths(&vec![PathBuf::from("x"); MAX_PATHS]).is_ok());
        assert!(matches!(
            quote_paths(&vec![PathBuf::from("x"); MAX_PATHS + 1]),
            Err(Error::FileDropSize)
        ));
        let text = "x".repeat(MAX_PASTE_BYTES - 2);
        assert_eq!(quote_paths(&[text.into()]).unwrap().len(), MAX_PASTE_BYTES);
        for text in [
            "x".repeat(MAX_PASTE_BYTES - 1),
            "'".repeat(MAX_PASTE_BYTES / 4),
            "\u{e9}".repeat(MAX_PASTE_BYTES / 2),
            "x".repeat(MAX_PASTE_BYTES + 1),
        ] {
            assert!(matches!(
                quote_paths(&[text.into()]),
                Err(Error::FileDropSize)
            ));
        }
        assert!(matches!(
            quote_paths(&["x".repeat(MAX_PASTE_BYTES - 4).into(), "y".into()]),
            Err(Error::FileDropSize)
        ));
    }

    #[test]
    fn empty_drop_is_no_text_but_empty_path_is_invalid() {
        assert_eq!(quote_paths(&[]).unwrap(), "");
        assert!(matches!(
            quote_paths(&[PathBuf::new()]),
            Err(Error::FileDropEmptyPath)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_non_utf8_instead_of_lossy_replacement() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_vec(b"/private/\xff".to_vec()));
        let error = quote_paths(&[path]).unwrap_err();
        assert!(matches!(error, Error::FileDropEncoding));
        assert!(!format!("{error} {error:?}").contains("private"));
    }

    fn prepare(view: &mut HerdrWindow) {
        // Reconnect only to the fixture's explicit nonexistent socket, never a
        // personal daemon. The projection below is entirely synthetic.
        view.reconnect();
        let snapshot = crate::sidebar::layout_tests::snapshot(2);
        let frame = FrameData {
            width: 80,
            height: 24,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        };
        view.options.surface_size.cols = frame.width;
        view.options.surface_size.rows = frame.height;
        view.live.surface = Some(Arc::new(PaneSurfaceFrame {
            boot_id: snapshot.boot_id.clone(),
            projection_revision: snapshot.revision,
            surface_revision: 1,
            frame,
            panes: [0, 40]
                .into_iter()
                .map(|x| PaneSurfacePane {
                    pane_id: format!("pane-{x}"),
                    content_revision: 1,
                    rect: SurfaceRect {
                        x,
                        y: 0,
                        width: 40,
                        height: 24,
                    },
                    inner_rect: SurfaceRect {
                        x: x + 1,
                        y: 1,
                        width: 38,
                        height: 22,
                    },
                    scrollbar_rect: None,
                    scroll: None,
                    focused: x == 0,
                    mouse_reporting: false,
                    sgr_pixel_mouse: false,
                    alternate_screen_active: false,
                    pixel_width: 380,
                    pixel_height: 440,
                })
                .collect(),
            splits: vec![],
            popup: None,
            graphics: Default::default(),
        }));
        view.live.snapshot = Some(Arc::new(snapshot));
        view.live.status = ConnectionStatus::Connected;
        view.cell_width = 10.;
        view.bounds = Bounds::new(
            point(px(100.), px(50.)),
            size(px(800.), px(24. * view.config.terminal.line_height())),
        );
        assert!(view.input_ready());
    }

    struct DropFixture {
        view: gpui_kit::Entity<HerdrWindow>,
        submitted_at: Option<Point<Pixels>>,
    }

    impl Render for DropFixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            gpui_kit::div().size_full().on_drop(cx.listener(
                |this, paths: &ExternalPaths, window, cx| {
                    this.submitted_at = Some(window.mouse_position());
                    this.view.update(cx, |view, cx| {
                        view.drop_terminal_files(paths, window, cx);
                    });
                },
            ))
        }
    }

    #[gpui_kit::test]
    fn external_drop_dispatches_with_submit_position_and_empty_payload_is_inert(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        // GPUI 0.2.2 exposes only Default for ExternalPaths construction; its
        // populated constructor is platform-private. Exercise actual dispatch
        // with an empty payload, and populated formatting separately above.
        let (fixture, cx) = crate::test_support::add_window_view(cx, |window, cx| DropFixture {
            view: cx.new(|cx| {
                let mut view = fixture_window(window, cx);
                prepare(&mut view);
                view
            }),
            submitted_at: None,
        });
        cx.update(|window, cx| {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("unchanged".into()));
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let entered = point(px(120.), px(100.));
        let submitted = point(px(520.), px(100.));
        cx.simulate_event(gpui_kit::FileDropEvent::Entered {
            position: entered,
            paths: ExternalPaths::default(),
        });
        cx.simulate_event(gpui_kit::FileDropEvent::Submit {
            position: submitted,
        });
        fixture.read_with(cx, |fixture, cx| {
            assert_eq!(fixture.submitted_at, Some(submitted));
            assert!(fixture.view.read(cx).local_error.is_none());
        });
        assert_eq!(
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text())),
            Some("unchanged".into())
        );
    }

    #[gpui_kit::test]
    fn targets_drop_position_not_focus_and_never_targets_chrome_or_covered_panes(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
        view.update(cx, |view, _| {
            prepare(view);
            let at = |column: f32, row: f32| {
                point(
                    px(100. + column * 10.),
                    px(50. + row * view.config.terminal.line_height()),
                )
            };
            assert_eq!(
                view.file_drop_target(at(42., 2.)),
                Some(InputTarget::Pane("pane-40".into()))
            );
            for position in [
                at(-1., 2.),
                at(0., 2.),
                at(40., 2.),
                at(42., 0.),
                at(81., 2.),
            ] {
                assert_eq!(view.file_drop_target(position), None);
            }
            let outside_popup = at(2., 2.);
            let inside_popup = at(32., 9.);
            let surface = Arc::make_mut(view.live.surface.as_mut().unwrap());
            surface.popup = Some(Box::new(ClientShellPopupSurface {
                terminal_id: "popup".into(),
                title: String::new(),
                width: None,
                height: None,
                frame: FrameData {
                    width: 20,
                    height: 10,
                    ..surface.frame.clone()
                },
                mouse_reporting: false,
                sgr_pixel_mouse: false,
                pixel_width: 200,
                pixel_height: 200,
            }));
            assert_eq!(view.file_drop_target(outside_popup), None);
            assert_eq!(
                view.file_drop_target(inside_popup),
                Some(InputTarget::Popup("popup".into()))
            );
        });
    }

    #[gpui_kit::test]
    fn drops_obey_menu_readiness_and_projection_fences(cx: &mut gpui_kit::TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
        view.update(cx, |view, _| {
            prepare(view);
            let position =
                view.bounds.origin + point(px(20.), px(2. * view.config.terminal.line_height()));
            assert!(view.file_drop_target(position).is_some());
            view.menu.page = Some(crate::menu::Page::Palette);
            assert_eq!(view.file_drop_target(position), None);
            view.menu.page = None;
            view.pending_toast = Some(1);
            assert_eq!(view.file_drop_target(position), None);
            view.pending_toast = None;
            view.options.surface_size.cols += 1;
            assert_eq!(view.file_drop_target(position), None);
            view.options.surface_size.cols -= 1;
            view.live.status = ConnectionStatus::Disconnected;
            assert_eq!(view.file_drop_target(position), None);
            view.live.status = ConnectionStatus::Connected;
            Arc::make_mut(view.live.surface.as_mut().unwrap()).projection_revision += 1;
            assert_eq!(view.file_drop_target(position), None);
            Arc::make_mut(view.live.surface.as_mut().unwrap()).projection_revision -= 1;
            Arc::make_mut(view.live.surface.as_mut().unwrap())
                .boot_id
                .push_str("-old");
            assert_eq!(view.file_drop_target(position), None);
        });
    }
}
