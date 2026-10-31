//! The dividers between groups. A divider draws a line one pixel wide, so
//! the groups beside it touch the line. The strip to drag it by is wider than
//! the line: the rest of it lies over the groups, inside their frames, and
//! over the pages they show on macOS.

use crate::{
    HerdrWindow,
    browser::Shown,
    panel_resize::{GRIP_THICKNESS, GripHold},
};
use gpui::{prelude::*, *};

/// How wide a divider's line draws.
const LINE: f32 = 1.;

/// How wide the strip to drag a divider by is, its line included.
const GRIP: f32 = 5.;

/// What a divider drags: the index of the group on its left. The row it
/// resizes reads the pointer.
#[derive(Clone, Copy)]
struct DividerDrag(usize);

/// Whether `shown` is a native page that draws over everything the app
/// draws, so a grip laid over it could not be pressed.
#[cfg(windows)]
pub(in crate::window) fn native_page(shown: &Shown, cx: &App) -> bool {
    let Shown::Page(id) = shown else {
        return false;
    };
    cx.try_global::<crate::browser::Store>()
        .and_then(|store| store.get(*id))
        .is_some_and(|tab| tab.location.as_ref().is_none_or(|at| at.is_page()))
}

/// Only Windows pages draw over the app. On macOS a page lies beneath the
/// app's drawing, and a grip over it takes the press; Linux shows no pages.
#[cfg(not(windows))]
pub(in crate::window) fn native_page(_: &Shown, _: &App) -> bool {
    false
}

/// How far the grips of a group's two dividers lie over the group, from its
/// left and right edges.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(in crate::window) struct Reach {
    pub(super) left: f32,
    pub(super) right: f32,
}

impl Reach {
    /// The reach over the `index`th group, given which groups show a native
    /// page. A grip is centered on its line between two groups the app draws.
    /// Beside one native page it lies wholly over the other group, so it
    /// keeps its width. Between two native pages only the line is left.
    pub(in crate::window) fn of(pages: &[bool], index: usize) -> Self {
        let Some(&page) = pages.get(index) else {
            return Self::default();
        };
        let toward = |neighbor: Option<usize>| {
            let neighbor = neighbor.and_then(|neighbor| pages.get(neighbor));
            match (page, neighbor) {
                (true, _) | (_, None) => 0.,
                (false, Some(true)) => GRIP - LINE,
                (false, Some(false)) => (GRIP - LINE) / 2.,
            }
        };
        Self {
            left: toward(index.checked_sub(1)),
            right: toward(index.checked_add(1)),
        }
    }

    /// The parts of the grips that lie over the `index`th group, for its
    /// frame, which must be `relative()`.
    pub(super) fn grips(
        self,
        index: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<Stateful<Div>> {
        let left = index
            .checked_sub(1)
            .filter(|_| self.left > 0.)
            .map(|divider| {
                hold(divider, window, cx).listen(
                    Piece::Right as u8,
                    grip(divider, "right", self.left).left_0(),
                )
            });
        let right = (self.right > 0.).then(|| {
            hold(index, window, cx)
                .listen(Piece::Left as u8, grip(index, "left", self.right).right_0())
        });
        left.into_iter().chain(right).collect()
    }
}

/// The pieces a divider is drawn in, which share one pill on its line.
enum Piece {
    /// The grip's part over the group on the line's left.
    Left,
    Line,
    /// The grip's part over the group on the line's right.
    Right,
}

/// The pointer's hold on the `divider`th divider, which its line and both
/// parts of its grip share, though they are drawn in different groups.
fn hold(divider: usize, window: &mut Window, cx: &mut App) -> GripHold {
    GripHold::keyed(("group-divider", divider), window, cx)
}

/// The part of the `divider`th divider's grip on one `side` of its line.
fn grip(divider: usize, side: &'static str, width: f32) -> Stateful<Div> {
    let selector = format!("group-divider-{divider}-{side}");
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector)
        .absolute()
        .top_0()
        .h_full()
        .w(px(width))
        .cursor(CursorStyle::ResizeLeftRight)
        // A press on the grip is not a press on what the group shows under
        // it. Blocking what lies beneath also keeps a page under the grip
        // from taking the press.
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_drag(DividerDrag(divider), |_, _, _, cx| cx.new(|_| EmptyView))
}

impl HerdrWindow {
    /// The groups in a row, with a divider between each pair that drags
    /// the two apart.
    pub(in crate::window) fn render_groups(
        &mut self,
        groups: Vec<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Real groups keep their layout index for the dividers between them;
        // folding ones slot in where they stood.
        let mut children: Vec<(Option<usize>, AnyElement)> = groups
            .into_iter()
            .enumerate()
            .map(|(index, group)| (Some(index), group))
            .collect();
        // Zen mode draws no group beside the one in use, folding or not.
        let mut folding = if self.zen_layout() {
            Vec::new()
        } else {
            self.folding_groups()
        };
        folding.sort_by_key(|folding| folding.index);
        for folding in folding {
            let at = folding.index.min(children.len());
            children.insert(at, (None, self.folding_group(&folding)));
        }
        let mut row = div()
            .id("groups")
            .flex()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<DividerDrag>, _, cx| {
                    let DividerDrag(divider) = *event.drag(cx);
                    let offset = f32::from(event.event.position.x - event.bounds.left());
                    if this.drag_divider(divider, offset, f32::from(event.bounds.size.width)) {
                        cx.notify();
                    }
                }),
            );
        let mut children = children.into_iter().peekable();
        while let Some((index, group)) = children.next() {
            row = row.child(group);
            // A divider sits between two real groups; a folding one draws its
            // own edge.
            if let (Some(index), Some((Some(_), _))) = (index, children.peek()) {
                let hold = hold(index, window, cx);
                let pill = hold.pill(Axis::Horizontal, false, window, cx);
                let line = div()
                    .id(("group-divider", index))
                    .debug_selector(move || format!("group-divider-{index}"))
                    .relative()
                    .flex_none()
                    .w(px(LINE))
                    .bg(rgb(self.theme.active))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_drag(DividerDrag(index), |_, _, _, cx| cx.new(|_| EmptyView));
                row = row.child(hold.listen(Piece::Line as u8, line).when_some(
                    pill,
                    |line, pill| {
                        // Wider than the line, the pill overhangs both
                        // groups; deferred, the group on the right does
                        // not paint over its half.
                        line.child(deferred(
                            div()
                                .absolute()
                                .top_0()
                                .h_full()
                                .left(px((LINE - GRIP_THICKNESS) / 2.))
                                .w(px(GRIP_THICKNESS))
                                .flex()
                                .items_center()
                                .child(pill),
                        ))
                    },
                ));
            }
        }
        row.into_any_element()
    }
}
