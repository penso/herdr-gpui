//! Staging a hunk from the review, so a commit can be built a hunk at a
//! time. Git does the work on the background executor (see `stage` in
//! `sheprd-vcs`), against the lines the review showed; the diff stays as it
//! was, since it compares with HEAD, and each hunk's button says whether
//! anything of it is left to stage. That comes from one listing of what is
//! unstaged, read with the review and after each hunk.
use super::{Review, State};
use crate::{
    SheprdWindow,
    browser::TabId,
    i18n::{Localize, fl},
    review::diff::{self, Kind, Scope, Status},
    stage::{self, Hunk},
    window::Flash,
};
use gpui::{prelude::*, *};
use std::{collections::HashMap, time::Instant};

/// A line of the file as the review cleans it.
fn clean(text: &str) -> String {
    let mut line = String::new();
    diff::push_clean(&mut line, text);
    line
}

impl Review {
    /// Whether the hunk whose header is line `header` of `file` is staged:
    /// nothing of it is left in the working tree alone. `None` for a hunk
    /// that cannot be staged as shown: a branch review holds commits, and
    /// one ignoring whitespace shows less than Git would stage.
    pub(super) fn hunk_staged(&self, file: usize, header: usize) -> Option<bool> {
        if self.scope != Scope::Uncommitted || self.ignore_whitespace {
            return None;
        }
        let entry = self.loaded()?.diff.files.get(file)?;
        let span = entry.lines()?.hunk_span(header)?;
        if entry.status == Status::Untracked {
            return Some(false);
        }
        // Until it is known, offer it: staging says when there is nothing.
        let Some(unstaged) = &self.unstaged else {
            return Some(false);
        };
        let left = unstaged
            .get(entry.git_path())
            .is_some_and(|spans| spans.iter().any(|other| other.touches(span)));
        Some(!left)
    }

    /// The hunk whose header is line `header` of `file`, as shown.
    fn stage_request(&self, file: usize, header: usize) -> Option<Hunk> {
        let entry = self.loaded()?.diff.files.get(file)?;
        let lines = entry.lines()?;
        let span = lines.hunk_span(header)?;
        let shown = lines
            .iter()
            .skip(header + 1)
            .take_while(|line| line.kind != Kind::Hunk)
            .filter_map(|line| Some((line.new?, lines.text_of(line).to_owned())))
            .collect::<HashMap<_, _>>();
        let removed = lines
            .iter()
            .skip(header + 1)
            .take_while(|line| line.kind != Kind::Hunk)
            .filter(|line| line.kind == Kind::Removed)
            .map(|line| lines.text_of(line).to_owned())
            .collect();
        Some(Hunk {
            path: entry.git_path().to_owned(),
            span,
            lines: shown,
            removed,
        })
    }
}

impl SheprdWindow {
    /// Reads what is left unstaged, for the hunks' buttons. Only the newest
    /// read lands; a failure, such as too much to list, leaves them offering
    /// to stage.
    pub(super) fn read_review_unstaged(&mut self, id: TabId, cx: &mut Context<Self>) {
        let Some(review) = self.reviews.get_mut(&id) else {
            return;
        };
        let State::Loaded(loaded) = &review.state else {
            return;
        };
        if loaded.scope != Scope::Uncommitted {
            return;
        }
        let checkout = loaded.source.checkout.clone();
        review.unstaged_read += 1;
        let read = review.unstaged_read;
        let reading = cx
            .background_executor()
            .spawn(async move { diff::unstaged(&checkout) });
        cx.spawn(async move |this, cx| {
            let unstaged = reading.await;
            let _ = this.update(cx, |this, cx| {
                let Some(review) = this
                    .reviews
                    .get_mut(&id)
                    .filter(|review| review.unstaged_read == read)
                else {
                    return;
                };
                review.unstaged = match unstaged {
                    Ok(unstaged) => Some(unstaged),
                    Err(error) => {
                        tracing::warn!(%error, "Could not read what is unstaged");
                        None
                    }
                };
                cx.notify();
            });
        })
        .detach();
    }

    /// Stages the hunk whose header is line `header` of `file`, one at a
    /// time. A file Git does not track yet is one hunk, and is added whole.
    pub(super) fn stage_review_hunk(
        &mut self,
        id: TabId,
        file: usize,
        header: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = self.reviews.get_mut(&id) else {
            return;
        };
        if review.staging || review.hunk_staged(file, header) != Some(false) {
            return;
        }
        let (Some(hunk), Some(loaded)) = (review.stage_request(file, header), review.loaded())
        else {
            return;
        };
        let checkout = loaded.source.checkout.clone();
        let untracked = loaded
            .diff
            .files
            .get(file)
            .is_some_and(|entry| entry.status == Status::Untracked);
        review.staging = true;
        let staging = cx.background_executor().spawn(async move {
            if untracked {
                stage::stage_file(&checkout, &hunk, clean)
            } else {
                stage::stage_hunk(&checkout, &hunk, clean)
            }
        });
        cx.spawn(async move |this, cx| {
            let staged = staging.await;
            let _ = this.update(cx, |this, cx| {
                this.review_hunk_staged(id, untracked, staged, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn review_hunk_staged(
        &mut self,
        id: TabId,
        untracked: bool,
        staged: sheprd_vcs::Result<()>,
        cx: &mut Context<Self>,
    ) {
        if let Some(review) = self.reviews.get_mut(&id) {
            review.staging = false;
        }
        match staged {
            Ok(()) => self.git.refresh_staged(Instant::now()),
            Err(error) => {
                self.git.refresh(Instant::now());
                self.show_flash(Flash::warning(error.localize()), cx);
            }
        }
        // A file just added is listed differently from then on.
        if untracked {
            self.reload_review(id, cx);
        } else {
            self.read_review_unstaged(id, cx);
        }
        cx.notify();
    }

    /// The hunk header's button that stages the hunk, or its word that the
    /// hunk is staged; nothing where a hunk cannot be.
    pub(super) fn review_hunk_stage(
        &self,
        id: TabId,
        review: &Review,
        file: usize,
        header: usize,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let theme = &self.theme;
        let staged = review.hunk_staged(file, header)?;
        let button = div()
            .id(ElementId::named_usize(
                format!("review-stage-hunk-{file}"),
                header,
            ))
            .debug_selector(move || format!("review-stage-hunk-{file}-{header}"))
            .flex_none()
            .px_1()
            .rounded(px(crate::config::corners::CONTROL));
        if staged {
            return Some(
                button
                    .debug_selector(move || format!("review-hunk-staged-{file}-{header}"))
                    .text_color(rgb(theme.ink(theme.palette[2])))
                    .child(fl!("review-hunk-staged")),
            );
        }
        Some(
            button
                .text_color(rgb(theme.muted))
                .when(!review.staging, |button| {
                    button
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.surface)))
                })
                .child(fl!("review-stage-hunk"))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.stage_review_hunk(id, file, header, cx);
                })),
        )
    }
}
