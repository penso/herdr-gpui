//! Opening and drawing a machine's tab, and the status bar item that says a
//! machine is running whether or not a tab shows it.

#[cfg(target_os = "macos")]
use super::TabId;
use super::{Location, Store, Tab};
use crate::{SheprdWindow, i18n::fl, machine::Name, window::Flash};
use gpui::{AnyElement, Context, Div, IntoElement, ParentElement, Styled, Window, div, rgb};
#[cfg(target_os = "macos")]
use gpui::{
    App, InteractiveElement, Stateful, StatefulInteractiveElement, WeakEntity,
    prelude::FluentBuilder as _, px, svg,
};

impl SheprdWindow {
    /// Shows machine `name` in a tab of the focused workspace, or brings back
    /// the one already showing it there, and starts it if it is stopped.
    pub(crate) fn open_machine(&mut self, name: Name, window: &mut Window, cx: &mut Context<Self>) {
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning(fl!("machine-no-workspace")), cx);
            return;
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store
                .in_workspace(&scope, &workspace)
                .find(|tab| matches!(&tab.location, Some(Location::Machine { name: open }) if *open == name))
                .map(|tab| tab.id)
        });
        let opened = existing.or_else(|| {
            Store::update(cx, {
                let name = name.clone();
                |store| store.open(scope, &workspace, Some(Location::Machine { name }), None)
            })
        });
        let Some(id) = opened else {
            self.show_flash(Flash::warning(fl!("machine-too-many-tabs")), cx);
            return;
        };
        self.dismiss_menu(window, cx);
        self.show_browser_tab(id, window, cx);
        #[cfg(target_os = "macos")]
        crate::machine::start(&name, cx);
        cx.notify();
    }

    /// The machine's display, or what it is doing instead.
    pub(crate) fn render_machine_tab(&self, tab: &Tab, cx: &mut Context<Self>) -> AnyElement {
        match self.machine_display(tab, cx) {
            Ok(display) => display,
            Err(notice) => notice,
        }
    }

    fn machine_notice(&self, message: String) -> Div {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .items_center()
            .justify_center()
            .px_4()
            .text_color(rgb(self.theme.muted))
            .child(message)
    }

    #[cfg(target_os = "macos")]
    fn machine_display(&self, tab: &Tab, cx: &mut Context<Self>) -> Result<AnyElement, AnyElement> {
        use crate::machine::{Status, status};
        if let Some(message) = self.browser.failed.get(&tab.id) {
            return Err(self.machine_notice(message.to_string()).into_any_element());
        }
        let Some(Location::Machine { name }) = &tab.location else {
            return Err(div().into_any_element());
        };
        let (message, startable) = match status(name) {
            Status::Running => {
                return Ok(match self.browser.pages.element(tab.id) {
                    Some(page) => page.into_any_element(),
                    None => div().size_full().into_any_element(),
                });
            }
            Status::Starting => (fl!("machine-starting", name = name.as_str()), false),
            Status::Saving => (fl!("machine-saving", name = name.as_str()), false),
            Status::Stopped => (fl!("machine-stopped", name = name.as_str()), true),
            Status::Failed(message) => (message, true),
        };
        let name = name.clone();
        Err(self
            .machine_notice(message)
            .when(startable, |notice| {
                notice.child(
                    div()
                        .id("machine-start")
                        .debug_selector(|| "machine-start".into())
                        .px_3()
                        .py_1()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgb(self.theme.active))
                        .text_color(rgb(self.theme.foreground))
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(self.theme.active)))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            crate::machine::start(&name, cx);
                        }))
                        .child(fl!("machine-start")),
                )
            })
            .into_any_element())
    }

    #[cfg(not(target_os = "macos"))]
    fn machine_display(&self, _: &Tab, _: &mut Context<Self>) -> Result<AnyElement, AnyElement> {
        Err(self
            .machine_notice(fl!("error-machine-unsupported"))
            .into_any_element())
    }

    /// Whether tab `id` is a machine with no picture to show. Its view stays
    /// out of the window, or it would take the clicks meant for the notice
    /// drawn in its place.
    #[cfg(target_os = "macos")]
    pub(super) fn machine_is_dark(&self, id: TabId, cx: &App) -> bool {
        cx.try_global::<Store>()
            .and_then(|store| store.get(id))
            .is_some_and(|tab| {
                matches!(&tab.location, Some(Location::Machine { name })
                    if crate::machine::status(name) != crate::machine::Status::Running)
            })
    }

    /// Says which machines are running, tab or no tab: each takes memory and
    /// CPU until it is suspended. Opens their settings.
    #[cfg(target_os = "macos")]
    pub(crate) fn render_machines_status(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let running = crate::machine::running();
        let label = match running.as_slice() {
            [] => return None,
            [only] => fl!("machine-status-one", name = only.as_str()),
            many => fl!("machine-status-many", count = many.len()),
        };
        let source = cx.entity().downgrade();
        Some(
            div()
                .id("status-machines")
                .debug_selector(|| "status-machines".into())
                .flex()
                .flex_row()
                .items_center()
                .gap(px(5.))
                .px_2()
                .flex_none()
                .cursor_pointer()
                .hover(|style| style.bg(rgb(self.theme.active)))
                .child(
                    svg()
                        .path("icons/computer.svg")
                        .size(px(14.))
                        .text_color(rgb(self.theme.primary())),
                )
                .child(label)
                .on_click(move |_, _, cx| {
                    crate::settings_window::open_machines(source.clone(), None, cx);
                }),
        )
    }
}

/// Opens machine `name` in the window `source` names, from the Settings
/// window, which has no tabs of its own.
#[cfg(target_os = "macos")]
pub(crate) fn open_in(source: &WeakEntity<SheprdWindow>, name: Name, cx: &mut App) {
    let source = source.clone();
    cx.defer(move |cx| {
        let windows = cx.windows();
        let window = windows
            .iter()
            .filter_map(|window| window.downcast::<SheprdWindow>())
            .find(|window| {
                window.read_with(cx, |_, _| ()).is_ok()
                    && window
                        .entity(cx)
                        .is_ok_and(|entity| entity.downgrade() == source)
            });
        if let Some(window) = window {
            let _ = window.update(cx, |view, window, cx| {
                window.activate_window();
                view.open_machine(name, window, cx);
            });
        }
    });
}
