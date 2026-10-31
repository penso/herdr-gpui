//! Keyboard focus in the window's chrome. Tab belongs to the terminal, so the
//! chrome is reached a region at a time: Focus Next Region (F6) moves from the
//! terminal to the title bar, the sidebar, the notice cards while any show,
//! the mod panel while it shows, the status bar, and back. Inside a
//! region Tab and Shift-Tab move between its controls, Enter or Space uses
//! one, and Escape returns to the terminal.
//!
//! Controls become tab stops only while their region holds the keyboard. A
//! focusable control takes focus on every mouse press, which would pull typing
//! away from the terminal each time a chrome button is clicked.

use super::HerdrWindow;
use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Region {
    TitleBar,
    Sidebar,
    /// The config and announcement cards over the terminal's top right.
    Notices,
    /// The panel of mod sections on the window's right (`crate::mods`).
    ModPanel,
    StatusBar,
}

/// The region after `current` in `regions`, `None` being the terminal, which
/// sits between the last region and the first.
fn step(regions: &[Region], current: Option<Region>, forward: bool) -> Option<Region> {
    let position = current.and_then(|region| regions.iter().position(|r| *r == region));
    match (position, forward) {
        (None, true) => regions.first().copied(),
        (None, false) => regions.last().copied(),
        (Some(index), true) => regions.get(index + 1).copied(),
        (Some(0), false) => None,
        (Some(index), false) => regions.get(index - 1).copied(),
    }
}

impl HerdrWindow {
    /// Leaves the region whenever the terminal takes focus back, by a click
    /// or otherwise, so the region's controls stop being tab stops and a
    /// later click on one cannot take typing from the terminal.
    pub(crate) fn leave_region_with_terminal(
        focus: &FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.on_focus(focus, window, |this, _, cx| {
            if this.chrome_region.take().is_some() {
                cx.notify();
            }
        })
    }

    /// The regions on screen, in reading order.
    fn regions(&self) -> Vec<Region> {
        let zen = self.zen_layout();
        [
            (Region::TitleBar, true),
            (Region::Sidebar, self.sidebar_visible && !zen),
            (Region::Notices, self.notices_shown()),
            (Region::ModPanel, self.mod_panel_shown()),
            (Region::StatusBar, self.status_bar_visible && !zen),
        ]
        .into_iter()
        .filter_map(|(region, shown)| shown.then_some(region))
        .collect()
    }

    /// The region holding the keyboard, if focus is still in it rather than
    /// back in the terminal after a click.
    pub(super) fn focused_region(&self, window: &Window) -> Option<Region> {
        self.chrome_region
            .filter(|_| !self.focus.is_focused(window))
            .filter(|region| self.regions().contains(region))
    }

    /// Returns the keyboard to the terminal once the region holding it is no
    /// longer drawn, as when Enter dismisses the last notice card: its focused
    /// control is gone, and Escape could no longer leave a region that is not
    /// on screen. An open menu page keeps its own focus.
    pub(super) fn leave_vanished_region(&mut self, window: &mut Window, cx: &mut App) {
        let Some(region) = self.chrome_region else {
            return;
        };
        if self.regions().contains(&region) {
            return;
        }
        self.chrome_region = None;
        if self.menu.page.is_none() {
            window.focus(&self.focus, cx);
        }
    }

    pub(super) fn cycle_region(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = step(&self.regions(), self.focused_region(window), forward);
        self.chrome_region = next;
        cx.notify();
        if next.is_none() {
            window.focus(&self.focus, cx);
            return;
        }
        // The region's controls become tab stops in the frame this change
        // draws, so the first of them is focused after it.
        let view = cx.entity().downgrade();
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                window.blur(cx);
                window.focus_next(cx);
                if window.focused(cx).is_none() {
                    // Nothing in the region could take focus.
                    let _ = view.update(cx, |this, cx| {
                        this.chrome_region = None;
                        window.focus(&this.focus, cx);
                    });
                }
            });
        });
    }

    /// Tab, Shift-Tab, and Escape while a region holds the keyboard; in the
    /// sidebar's lists the arrows move too.
    pub(super) fn chrome_focus_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(region) = self.focused_region(window) else {
            return;
        };
        if self.menu.page.is_some() || self.menu.application.is_some() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let plain = modifiers == Modifiers::default();
        let shift = modifiers == Modifiers::shift();
        match event.keystroke.key.as_str() {
            "tab" if plain => window.focus_next(cx),
            "tab" if shift => window.focus_prev(cx),
            "down" if plain && region == Region::Sidebar => window.focus_next(cx),
            "up" if plain && region == Region::Sidebar => window.focus_prev(cx),
            "escape" if plain => {
                self.chrome_region = None;
                window.focus(&self.focus, cx);
                cx.notify();
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    /// Makes `control` a tab stop with a focus ring while `region` holds the
    /// keyboard; otherwise leaves it as it was.
    pub(crate) fn chrome_stop<E: InteractiveElement>(&self, region: Region, control: E) -> E {
        if self.chrome_region != Some(region) {
            return control;
        }
        let ring = rgb(self.theme.primary());
        control.tab_index(0).focus_visible(move |style| {
            style.shadow(vec![BoxShadow {
                color: ring.into(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: true,
            }])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Region, step};
    use crate::sidebar::layout_tests::{fixture_window, full_draw};

    #[gpui::test]
    fn clicking_back_into_the_terminal_leaves_the_region(cx: &mut gpui::TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        // Focus events reach listeners only in the active window.
        cx.update(|window, _| window.activate_window());
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                window.focus(&view.sidebar_search.focus(cx), cx);
                view.chrome_region = Some(Region::Sidebar);
            });
            full_draw(window, cx).clear(cx);
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.chrome_region),
            Some(Region::Sidebar)
        );
        cx.update(|window, cx| {
            let terminal = view.read(cx).focus.clone();
            window.focus(&terminal, cx);
            full_draw(window, cx).clear(cx);
        });
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |view, _| view.chrome_region), None);
    }

    #[test]
    fn regions_cycle_through_the_terminal_both_ways() {
        let all = [Region::TitleBar, Region::Sidebar, Region::StatusBar];
        let mut forward = vec![];
        let mut at = None;
        for _ in 0..4 {
            at = step(&all, at, true);
            forward.push(at);
        }
        assert_eq!(
            forward,
            [
                Some(Region::TitleBar),
                Some(Region::Sidebar),
                Some(Region::StatusBar),
                None
            ]
        );
        assert_eq!(step(&all, None, false), Some(Region::StatusBar));
        assert_eq!(step(&all, Some(Region::TitleBar), false), None);
    }

    #[test]
    fn a_hidden_region_is_skipped() {
        let shown = [Region::TitleBar, Region::StatusBar];
        assert_eq!(
            step(&shown, Some(Region::TitleBar), true),
            Some(Region::StatusBar)
        );
        // A region that has since hidden starts over from the terminal.
        assert_eq!(
            step(&shown, Some(Region::Sidebar), true),
            Some(Region::TitleBar)
        );
    }
}
