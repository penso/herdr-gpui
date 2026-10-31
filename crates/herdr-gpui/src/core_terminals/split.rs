//! Drawing a herdr-core tab: one pane bare, as a Herdr tab draws, or a split
//! in the tab layout's [`PaneChrome`], with a line between two panes to drag.

use super::{
    State,
    panes::{Axis, Node, Share},
    view::PaneLook,
};
use crate::{
    HerdrWindow,
    browser::{Slot, Tab, TabId},
    i18n::fl,
    sidebar::Indicators,
    tab_layout::panes::{PaneChrome, PaneContext, PaneFace},
};
use gpui::{prelude::*, *};
use herdr_backend::TerminalId;
use herdr_client::protocol::AgentStatus;

/// How wide the strip that drags a split is, whatever its gutter.
const GRAB: f32 = 6.;

/// What drawing one tab's panes shares.
struct Draw<'a> {
    tab: TabId,
    chrome: &'static dyn PaneChrome,
    panes: PaneContext<'a>,
    active: TerminalId,
    /// A zoomed pane drawn over the split, left out of it.
    skip: Option<TerminalId>,
}

/// A split's line being dragged: the tab, the split's place in its tree,
/// and which way it divides.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SplitDrag {
    tab: TabId,
    path: Vec<bool>,
    axis: Axis,
}

impl HerdrWindow {
    pub(crate) fn render_core_terminal_tab(
        &mut self,
        _slot: Slot,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(shown) = self.core_terminals.tabs.get(&tab.id) else {
            return self.core_placeholder();
        };
        if shown.panes.len() == 1 {
            let key = (tab.id, shown.panes.first().clone());
            let Some(view) = self.core_terminals.views.get(&key) else {
                return self.core_placeholder();
            };
            view.update(cx, |view, _| view.set_pane(None));
            return view.clone().into_any_element();
        }
        let zoom = shown
            .zoom_at(crate::motion::now())
            .map(|(zoomed, shown)| (zoomed.clone(), shown));
        let rects = shown.panes.rects();
        let root = shown.panes.root().clone();
        let active = shown.active.clone();
        let chrome = self.tab_layout().panes();
        let indicators = self.core_indicators(cx);
        let draw = Draw {
            tab: tab.id,
            chrome,
            panes: PaneContext {
                theme: &self.theme,
                tabs: &self.config.tabs,
                terminal: &self.config.terminal,
                indicators,
            },
            active,
            skip: zoom.as_ref().map(|(zoomed, _)| zoomed.clone()),
        };
        // A zoom that has landed hides the split; one in motion grows over it
        // from the pane's place, and shrinks back to it.
        let landed = zoom.as_ref().is_some_and(|(_, shown)| *shown >= 1.);
        let split = (!landed).then(|| self.core_node(&draw, &root, &mut Vec::new(), cx));
        let zoomed = zoom.and_then(|(zoomed, shown)| {
            let (_, from) = rects.iter().find(|(pane, _)| *pane == zoomed)?;
            let lerp = |from: f32, to: f32| from + (to - from) * shown;
            let pane = self.core_leaf(&draw, &zoomed, cx);
            Some(
                div()
                    .absolute()
                    .left(relative(lerp(from.x, 0.)))
                    .top(relative(lerp(from.y, 0.)))
                    .w(relative(lerp(from.width, 1.)))
                    .h(relative(lerp(from.height, 1.)))
                    .flex()
                    .child(pane),
            )
        });
        div()
            .size_full()
            .flex()
            .p(px(chrome.inset()))
            .bg(rgb(chrome.gutter_fill(&self.theme)))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .children(split)
                    .children(zoomed),
            )
            .into_any_element()
    }

    /// `node` of the tab's tree, at `path` from its root.
    fn core_node(
        &self,
        draw: &Draw<'_>,
        node: &Node,
        path: &mut Vec<bool>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (tab, chrome) = (draw.tab, draw.chrome);
        let (axis, share, first, second) = match node {
            Node::Leaf(terminal) if draw.skip.as_ref() == Some(terminal) => {
                return div().flex_1().into_any_element();
            }
            Node::Leaf(terminal) => return self.core_leaf(draw, terminal, cx),
            Node::Split {
                axis,
                share,
                first,
                second,
            } => (*axis, *share, first, second),
        };
        path.push(false);
        let first = self.core_node(draw, first, path, cx);
        path.pop();
        path.push(true);
        let second = self.core_node(draw, second, path, cx);
        path.pop();
        let fraction = share.fraction();
        let gutter = chrome.gutter();
        let drag = SplitDrag {
            tab,
            path: path.clone(),
            axis,
        };
        let id: String = path
            .iter()
            .map(|side| if *side { '1' } else { '0' })
            .collect();
        let first = div()
            .flex()
            .flex_none()
            .min_w_0()
            .min_h_0()
            .map(|part| match axis {
                Axis::Row => part.w(relative(fraction)),
                Axis::Column => part.h(relative(fraction)),
            })
            .child(first);
        let gap = div()
            .flex_none()
            .bg(rgb(chrome.gutter_fill(&self.theme)))
            .map(|gap| match axis {
                Axis::Row => gap.w(px(gutter)),
                Axis::Column => gap.h(px(gutter)),
            });
        let second = div().flex().flex_1().min_w_0().min_h_0().child(second);
        // Laid over the gutter, wider than a hairline one, and after both
        // panes so neither covers it.
        let grab = div()
            .id(SharedString::from(format!(
                "core-split-{}-{id}",
                u64::from(tab)
            )))
            .absolute()
            .map(|grab| match axis {
                Axis::Row => grab
                    .top_0()
                    .bottom_0()
                    .left(relative(fraction))
                    .ml(px((gutter - GRAB) / 2.))
                    .w(px(GRAB))
                    .cursor(CursorStyle::ResizeLeftRight),
                Axis::Column => grab
                    .left_0()
                    .right_0()
                    .top(relative(fraction))
                    .mt(px((gutter - GRAB) / 2.))
                    .h(px(GRAB))
                    .cursor(CursorStyle::ResizeUpDown),
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_drag(drag.clone(), |_, _, _, cx| cx.new(|_| EmptyView));
        div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .when(axis == Axis::Column, |split| split.flex_col())
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<SplitDrag>, _, cx| {
                    if *event.drag(cx) != drag {
                        return;
                    }
                    let bounds = event.bounds;
                    let position = event.event.position;
                    let fraction = match drag.axis {
                        Axis::Row => {
                            f32::from(position.x - bounds.origin.x)
                                / f32::from(bounds.size.width).max(1.)
                        }
                        Axis::Column => {
                            f32::from(position.y - bounds.origin.y)
                                / f32::from(bounds.size.height).max(1.)
                        }
                    };
                    if this.set_core_share(&drag, Share::at(fraction)) {
                        cx.notify();
                    }
                }),
            )
            .child(first)
            .child(gap)
            .child(second)
            .child(grab)
            .into_any_element()
    }

    fn core_leaf(
        &self,
        draw: &Draw<'_>,
        terminal: &TerminalId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (tab, chrome) = (draw.tab, draw.chrome);
        let Some(view) = self.core_terminals.views.get(&(tab, terminal.clone())) else {
            return div().flex_1().into_any_element();
        };
        let is_active = *terminal == draw.active;
        view.update(cx, |view, _| {
            view.set_pane(Some(PaneLook {
                corners: chrome.corners(),
                veil: (!is_active).then(|| chrome.veil()),
            }))
        });
        let info = self.core_terminals.infos.get(terminal);
        let shown = view.read(cx);
        let (title, status) = self.core_pane_label(tab, terminal, cx);
        let face = PaneFace {
            title: title.into(),
            folder: info
                .and_then(|info| info.cwd.as_deref())
                .map(|cwd| home_relative(cwd).into()),
            status,
            size: shown
                .size()
                .map(|size| format!("{}\u{d7}{}", size.cols(), size.rows()).into()),
            active: is_active,
        };
        chrome.pane(face, view.clone().into_any_element(), &draw.panes)
    }

    /// What a pane is called and how its program is doing, as its chrome
    /// and the sidebar show it: the program's title, else herdr-core's, its
    /// name, or its folder for a shell that sets no title.
    pub(super) fn core_pane_label(
        &self,
        tab: TabId,
        terminal: &TerminalId,
        cx: &App,
    ) -> (String, AgentStatus) {
        let info = self.core_terminals.infos.get(terminal);
        let view = self
            .core_terminals
            .views
            .get(&(tab, terminal.clone()))
            .map(|view| view.read(cx));
        let title = view
            .and_then(|view| view.title())
            .map(str::to_owned)
            .or_else(|| info.and_then(|info| info.title.clone()))
            .or_else(|| info.and_then(|info| info.name.clone()))
            .or_else(|| {
                info.and_then(|info| info.cwd.as_ref())
                    .and_then(|cwd| cwd.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| terminal.to_string());
        let status = match view.is_some_and(|view| view.ended()) {
            true => AgentStatus::Done,
            false => info.map_or(AgentStatus::Unknown, |info| info.agent),
        };
        (title, status)
    }

    /// The status dots as the window's tabs and sidebar draw them.
    pub(super) fn core_indicators(&self, cx: &App) -> Indicators {
        Indicators::new(
            self.settings.shared.as_ref(),
            self.config.status_icons,
            self.config.edge_rail,
            matches!(
                cx.window_appearance(),
                WindowAppearance::Light | WindowAppearance::VibrantLight
            ),
            &self.theme,
        )
    }

    /// Moves the split a drag holds; true when it moved.
    fn set_core_share(&mut self, drag: &SplitDrag, share: Share) -> bool {
        let Some(shown) = self.core_terminals.tabs.get_mut(&drag.tab) else {
            return false;
        };
        let moved = shown.panes.set_share(&drag.path, share);
        shown.unsaved |= moved;
        moved
    }

    /// What a herdr-core tab shows before its panes have views.
    fn core_placeholder(&self) -> AnyElement {
        let note = match self.core_terminals.hub.state() {
            State::Unavailable(error) => {
                let reason = error.to_string();
                fl!("core-terminals-unavailable", reason = reason)
            }
            _ => fl!("core-terminals-connecting"),
        };
        div()
            .flex_1()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(self.fill().base(self.terminal_theme().background))
            .text_color(rgb(self.theme.foreground))
            .text_sm()
            .child(note)
            .into_any_element()
    }
}

/// `cwd` from the home folder, as a shell prompt shows it.
fn home_relative(cwd: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match home.as_deref().and_then(|home| cwd.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => cwd.display().to_string(),
    }
}
