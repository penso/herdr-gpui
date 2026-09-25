//! One sidebar row: a kit sidebar menu item whose icon carries the daemon's
//! activity status, with pull request, uncommitted work, owner avatar, and the
//! fold control as its suffix. The kit truncates labels to the row, so nothing
//! here measures glyphs.

use crate::HerdrWindow;
use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, ThemeColor,
    avatar::Avatar,
    button::{Button, ButtonVariants},
    h_flex,
    sidebar::SidebarMenuItem,
    spinner::Spinner,
    tag::Tag,
};
use gpui_kit::{prelude::*, *};
use herdr_client::protocol::AgentStatus;
use std::sync::Arc;

/// The theme's semantic color for an activity status. Status only ever comes
/// from the daemon's snapshot, so every client paints the same color.
pub(super) fn status_color(status: AgentStatus, theme: &ThemeColor) -> Hsla {
    match status {
        AgentStatus::Working => theme.warning,
        AgentStatus::Blocked => theme.danger,
        AgentStatus::Done => theme.info,
        AgentStatus::Idle => theme.success,
        AgentStatus::Unknown => theme.muted_foreground,
    }
}

/// A row's leading icon, tinted with the activity status it reports.
pub(super) fn status_icon(icon: impl Into<Icon>, status: AgentStatus, cx: &App) -> Icon {
    Icon::new(icon)
        .size_4()
        .text_color(status_color(status, cx.theme()))
}

/// Cached pull request state for a worktree row: its number, in the color of
/// its lifecycle and readiness.
#[derive(Clone)]
pub(super) struct PrTag {
    pub(super) number: SharedString,
    pub(super) color: Hsla,
}

impl PrTag {
    pub(super) fn new(pr: &crate::pull_request::PullRequest, theme: &crate::config::Theme) -> Self {
        Self {
            number: format!("#{}", pr.number).into(),
            color: rgb(pr.color(theme)).into(),
        }
    }
}

/// What a fold control hides: a host's rows, or a repository's worktrees.
#[derive(Clone)]
pub(super) enum FoldTarget {
    Host,
    Repo(String),
}

/// A fold control. Folding changes this client's view only, so the state lives
/// in the window (`collapsed_repos`, `Endpoint::collapsed`), not in the kit
/// item, whose own submenu state the workspace menu could not reach.
#[derive(Clone)]
pub(super) struct Fold {
    pub(super) id: SharedString,
    pub(super) collapsed: bool,
    pub(super) view: WeakEntity<HerdrWindow>,
    pub(super) endpoint: String,
    pub(super) target: FoldTarget,
}

/// Everything a row shows after its label, in the order it is laid out.
#[derive(Clone, Default)]
pub(super) struct RowSuffix {
    pub(super) note: Option<SharedString>,
    pub(super) removing: bool,
    pub(super) dirty: bool,
    pub(super) pr: Option<PrTag>,
    pub(super) avatar: Option<Arc<Image>>,
    pub(super) fold: Option<Fold>,
}

impl RowSuffix {
    fn is_empty(&self) -> bool {
        self.note.is_none()
            && !self.removing
            && !self.dirty
            && self.pr.is_none()
            && self.avatar.is_none()
            && self.fold.is_none()
    }

    fn render(&self, cx: &App) -> Div {
        let theme = cx.theme();
        h_flex()
            .flex_none()
            .gap_1()
            .when_some(self.note.clone(), |row, note| {
                row.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(note),
                )
            })
            .when(self.removing, |row| {
                row.child(
                    div()
                        .debug_selector(|| "worktree-removing".into())
                        .child(Spinner::new().xsmall().color(theme.primary)),
                )
            })
            // Uncommitted work: the pull request beside it counts the branch's
            // diff, not the working tree's.
            .when(self.dirty, |row| {
                row.child(
                    Icon::empty()
                        .path("icons/pencil.svg")
                        .xsmall()
                        .text_color(theme.warning),
                )
            })
            .when_some(self.pr.clone(), |row, pr| {
                row.child(
                    Tag::secondary()
                        .small()
                        .text_color(pr.color)
                        .child(pr.number),
                )
            })
            .when_some(self.avatar.clone(), |row, image| {
                row.child(Avatar::new().src(image).xsmall())
            })
            .when_some(self.fold.clone(), |row, fold| {
                let Fold {
                    id,
                    collapsed,
                    view,
                    endpoint,
                    target,
                } = fold;
                row.child(
                    Button::new(id)
                        .ghost()
                        .xsmall()
                        .icon(if collapsed {
                            IconName::ChevronRight
                        } else {
                            IconName::ChevronDown
                        })
                        .on_click(move |_, _, cx| {
                            // Folding must not also select the row under it.
                            cx.stop_propagation();
                            let _ = view
                                .update(cx, |this, cx| this.toggle_fold(&endpoint, &target, cx));
                        }),
                )
            })
    }
}

/// A kit sidebar item for one row. `size` is the configured sidebar font size,
/// which the kit's own small text would otherwise override.
pub(super) fn item(
    label: impl Into<SharedString>,
    icon: Option<Icon>,
    active: bool,
    suffix: RowSuffix,
    size: f32,
) -> SidebarMenuItem {
    let item = SidebarMenuItem::new(label)
        .active(active)
        .text_size(px(size))
        .when_some(icon, |item, icon| item.icon(icon));
    if suffix.is_empty() {
        return item;
    }
    item.suffix(move |_, cx| suffix.render(cx))
}

/// Four digits of churn is already a big diff; abbreviate past that so the
/// column stays narrow enough to leave the branch readable. The titlebar's Git
/// badge reuses it so one PR reads the same in both places.
pub(crate) fn compact(lines: u64) -> String {
    match lines {
        0..=9999 => lines.to_string(),
        _ => format!("{}k", lines / 1000),
    }
}

pub(super) fn first_text<'a>(
    values: impl IntoIterator<Item = Option<&'a str>>,
    fallback: &'a str,
) -> &'a str {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(fallback)
}

impl HerdrWindow {
    /// Folds or unfolds a host or one repository group on one endpoint.
    fn toggle_fold(&mut self, endpoint: &str, target: &FoldTarget, cx: &mut Context<Self>) {
        let local = endpoint == crate::endpoint::LOCAL;
        let Some(endpoint) = self.endpoints.iter_mut().find(|e| e.id == endpoint) else {
            return;
        };
        match target {
            FoldTarget::Host => endpoint.collapsed = !endpoint.collapsed,
            FoldTarget::Repo(key) => {
                let collapsed = if local {
                    &mut self.collapsed_repos
                } else {
                    &mut endpoint.collapsed_repos
                };
                if !collapsed.remove(key) {
                    collapsed.insert(key.clone());
                }
            }
        }
        cx.notify();
    }
}
