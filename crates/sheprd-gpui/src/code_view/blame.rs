//! Blame in a code tab: who last changed each line and in which commit,
//! read on the background executor from the text the tab read, so the two
//! always match. A
//! column beside the line numbers names each run of lines' commit, and a bar
//! under the code gives the whole of the marked line's.

use super::CodeView;
use crate::{
    SheprdWindow,
    blame::{Blame, Commit},
    browser::TabId,
    i18n::{Localize, fl},
    window::Flash,
};
use gpui::{prelude::*, *};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// The blame column's width in characters, and its share of a narrow tab.
const COLUMN_CHARS: f32 = 36.;
const MAX_COLUMN_SHARE: f32 = 0.4;

/// What a tab knows of its lines' commits. Blame stays shown while it is
/// read again.
pub(super) enum Lines {
    Off,
    Reading,
    Shown(Arc<Blame>),
}

impl Lines {
    /// Whether the user asked for it, read yet or not.
    pub(super) fn wanted(&self) -> bool {
        !matches!(self, Self::Off)
    }

    pub(super) fn shown(&self) -> Option<&Blame> {
        match self {
            Self::Shown(blame) => Some(blame),
            Self::Off | Self::Reading => None,
        }
    }
}

impl CodeView {
    pub(super) fn stop_blame(&self) {
        self.blame_cancelled.store(true, Ordering::Release);
    }
}

/// A commit as a line of text: its ID and author, or that it is not one.
fn name(commit: &Commit) -> String {
    if commit.uncommitted() {
        fl!("code-view-blame-uncommitted")
    } else {
        format!("{} {}", commit.id, commit.author)
    }
}

fn ago(commit: &Commit) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_secs()).ok())
        .unwrap_or(0);
    crate::ago::text(crate::ago::since(commit.time, now))
}

impl SheprdWindow {
    /// Shows or hides who last changed each line. Showing reads the file
    /// again too, so the commits are those of the lines on screen.
    pub(super) fn toggle_code_blame(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(view) = self.code_views.get_mut(&id) else {
            return;
        };
        if view.blame.wanted() {
            view.stop_blame();
            view.blame = Lines::Off;
            cx.notify();
            return;
        }
        view.blame = Lines::Reading;
        self.load_code_view(id, cx);
        cx.notify();
    }

    /// Whether tab `id` wants its lines' commits.
    pub(super) fn code_blame_wanted(&self, id: TabId) -> bool {
        self.code_views
            .get(&id)
            .is_some_and(|view| view.blame.wanted())
    }

    /// Blames `contents`, read `load` of `path`, stopping any older read.
    pub(super) fn read_code_blame(
        &mut self,
        id: TabId,
        load: u64,
        path: PathBuf,
        contents: String,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self.code_views.get_mut(&id) else {
            return;
        };
        view.stop_blame();
        let cancelled: Arc<AtomicBool> = Arc::default();
        view.blame_cancelled = cancelled.clone();
        let reading = cx
            .background_executor()
            .spawn(async move { Blame::read(&path, &contents, &cancelled).map(Arc::new) });
        cx.spawn(async move |this, cx| {
            let read = reading.await;
            let _ = this.update(cx, |this, cx| this.code_blame_read(id, load, read, cx));
        })
        .detach();
    }

    fn code_blame_read(
        &mut self,
        id: TabId,
        load: u64,
        read: sheprd_vcs::Result<Arc<Blame>>,
        cx: &mut Context<Self>,
    ) {
        let Some(view) = self
            .code_views
            .get_mut(&id)
            .filter(|view| view.load == load && view.blame.wanted())
        else {
            return;
        };
        match read {
            Ok(blame) => view.blame = Lines::Shown(blame),
            Err(error) => {
                view.blame = Lines::Off;
                self.show_flash(Flash::warning(error.localize()), cx);
            }
        }
        cx.notify();
    }

    /// How wide the blame column draws, when it shows.
    pub(super) fn code_blame_width(&self, view: &CodeView) -> Option<f32> {
        view.blame.shown()?;
        let wide = COLUMN_CHARS * self.config.terminal.size * 0.62;
        let room = view.width.get() * MAX_COLUMN_SHARE;
        Some(if room > 0. { wide.min(room) } else { wide })
    }

    /// Line `row`'s place in the blame column: the commit where a run of
    /// its lines starts, so a long run reads as one.
    pub(super) fn code_blame_cell(&self, view: &CodeView, row: usize, width: f32) -> Div {
        let theme = &self.theme;
        let cell = div()
            .debug_selector(move || format!("code-blame-{row}"))
            .flex_none()
            .w(px(width))
            .pr(px(12.))
            .flex()
            .gap(px(8.))
            .overflow_hidden()
            .text_color(rgb(theme.muted));
        let Some((blame, commit)) = view
            .blame
            .shown()
            .and_then(|blame| Some((blame, blame.line(row)?)))
        else {
            return cell;
        };
        if !blame.starts_run(row) {
            return cell;
        }
        cell.child(div().flex_1().min_w_0().truncate().child(name(commit)))
            .when(!commit.uncommitted(), |cell| {
                cell.child(div().flex_none().child(ago(commit)))
            })
    }

    /// The marked line's commit in full, under the code.
    pub(super) fn code_blame_bar(&self, view: &CodeView) -> Option<Div> {
        let theme = &self.theme;
        let row = usize::try_from(view.marked?.checked_sub(1)?).ok()?;
        let commit = view.blame.shown()?.line(row)?;
        let details = if commit.uncommitted() {
            name(commit)
        } else {
            format!(
                "{} \u{00b7} {} \u{00b7} {}",
                name(commit),
                ago(commit),
                commit.summary
            )
        };
        Some(
            div()
                .debug_selector(|| "code-blame-bar".into())
                .flex_none()
                .px(px(10.))
                .py(px(4.))
                .border_t_1()
                .border_color(rgb(theme.active))
                .text_color(rgb(theme.muted))
                .truncate()
                .child(details),
        )
    }
}
