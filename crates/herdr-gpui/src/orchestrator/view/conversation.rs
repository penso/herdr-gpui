//! A pull request's conversation, drawn the way GitHub reads: each comment a
//! card under its author's avatar, each review holding the inline threads
//! its author left, resolved threads folded to a line, and a summary of the
//! verdicts on top. Bodies are Markdown, parsed once per text, and drawn as
//! list rows so a long conversation scrolls at the cost of what is in view;
//! avatars are read off the UI thread and only ever from GitHub's avatar host.

mod rows;
mod scrollbar;

pub(super) use rows::Rows;

use super::{
    OrchestratorView,
    look::{BLUE, CYAN, GREEN, Look, MAGENTA, RED, YELLOW},
};
use crate::{
    config::{corners, mix},
    i18n::fl,
    pr_actions::{self, Comment, CommentKind, Review},
    pull_request::State,
    release_notes,
};
use gpui::{prelude::*, *};
use rows::{Card, Chrome, Gutter, Piece};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

const AVATAR: f32 = 32.;
/// Avatars kept per view; a conversation holds at most a few dozen authors.
const AVATAR_LIMIT: usize = 128;
/// How long a background read waits for a fresh avatar to download.
const AVATAR_WAIT: Duration = Duration::from_secs(6);

/// One block of the conversation.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Entry<'a> {
    /// A comment on the conversation itself.
    Comment(&'a Comment),
    /// A review, or the inline threads of an author who left no review
    /// entry, with the threads it holds in the order they were opened.
    Review {
        author: &'a str,
        review: Option<&'a Comment>,
        threads: Vec<&'a Comment>,
    },
}

impl Entry<'_> {
    fn at(&self) -> &str {
        match self {
            Self::Comment(comment) => &comment.created_at,
            Self::Review {
                review: Some(review),
                ..
            } => &review.created_at,
            Self::Review { threads, .. } => threads.first().map_or("", |t| &t.created_at),
        }
    }
}

/// The comments as blocks. A thread joins its author's first review
/// submitted at or after it opened, since inline comments are written
/// before their review is submitted; threads with no such review gather
/// under one block of their author's.
pub(super) fn entries(comments: &[Comment]) -> Vec<Entry<'_>> {
    let mut entries: Vec<Entry> = comments
        .iter()
        .filter_map(|comment| match comment.kind {
            CommentKind::Conversation => Some(Entry::Comment(comment)),
            CommentKind::Review(_) => Some(Entry::Review {
                author: &comment.author,
                review: Some(comment),
                threads: Vec::new(),
            }),
            CommentKind::Thread { .. } => None,
        })
        .collect();
    for thread in comments
        .iter()
        .filter(|comment| matches!(comment.kind, CommentKind::Thread { .. }))
    {
        let reviewed = entries.iter().position(|entry| {
            matches!(entry, Entry::Review { author, review: Some(review), .. }
                if *author == thread.author && review.created_at >= thread.created_at)
        });
        let gathered = || {
            entries.iter().position(|entry| {
                matches!(entry, Entry::Review { author, review: None, .. } if *author == thread.author)
            })
        };
        match reviewed.or_else(gathered) {
            Some(index) => {
                if let Entry::Review { threads, .. } = &mut entries[index] {
                    threads.push(thread);
                }
            }
            None => entries.push(Entry::Review {
                author: &thread.author,
                review: None,
                threads: vec![thread],
            }),
        }
    }
    // RFC 3339 UTC text orders by time; the sort keeps ties in place.
    entries.sort_by(|a, b| a.at().cmp(b.at()));
    entries
}

/// The counts the summary bar shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub(super) approved: usize,
    pub(super) changes_requested: usize,
    pub(super) open_threads: usize,
    pub(super) resolved_threads: usize,
    pub(super) comments: usize,
    pub(super) people: usize,
}

/// Each reviewer counts once, by their newest verdict.
pub(super) fn summary(comments: &[Comment]) -> Summary {
    let mut verdicts: HashMap<&str, Review> = HashMap::new();
    let mut summary = Summary {
        comments: comments.len(),
        people: comments
            .iter()
            .map(|comment| comment.author.as_str())
            .collect::<HashSet<_>>()
            .len(),
        ..Summary::default()
    };
    for comment in comments {
        match &comment.kind {
            CommentKind::Review(review @ (Review::Approved | Review::ChangesRequested)) => {
                verdicts.insert(&comment.author, *review);
            }
            CommentKind::Review(Review::Dismissed) => {
                verdicts.remove(comment.author.as_str());
            }
            CommentKind::Thread { resolved: true, .. } => summary.resolved_threads += 1,
            CommentKind::Thread { .. } => summary.open_threads += 1,
            _ => {}
        }
    }
    for verdict in verdicts.values() {
        match verdict {
            Review::Approved => summary.approved += 1,
            _ => summary.changes_requested += 1,
        }
    }
    summary
}

/// "2 h ago" from an RFC 3339 time, or nothing when it does not parse.
pub(super) fn ago(at: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| super::look::ago(Some(at.with_timezone(&chrono::Utc)), now))
        .unwrap_or_default()
}

/// Avatars by URL: `None` while being read or when none could be.
pub(super) type Avatars = HashMap<String, Option<Arc<Image>>>;

impl OrchestratorView {
    /// Reads the avatars of the conversation's authors not asked for yet,
    /// each on the background executor: the disk cache and the download
    /// both stay off the UI thread.
    pub(super) fn fetch_avatars(&mut self, cx: &mut Context<Self>) {
        let Some(comments) = self.pr.comments() else {
            return;
        };
        let wanted: Vec<String> = comments
            .iter()
            .filter_map(|comment| comment.avatar.clone())
            .filter(|url| !self.avatars.contains_key(url))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        for url in wanted {
            if self.avatars.len() >= AVATAR_LIMIT {
                return;
            }
            self.avatars.insert(url.clone(), None);
            let read = {
                let url = url.clone();
                cx.background_executor().spawn(async move {
                    let (image, updates) = crate::avatars::profile_avatar(&url);
                    // A stale cached image shows rather than waiting on GitHub.
                    image.or_else(|| updates?.recv_timeout(AVATAR_WAIT).ok())
                })
            };
            cx.spawn(async move |this, cx| {
                let image = read.await;
                let _ = this.update(cx, |this, cx| {
                    if image.is_some() {
                        this.avatars.insert(url, image);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    pub(super) fn render_conversation(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (notice, comments) = if self.request().is_none_or(|request| request.token.is_none()) {
            (Some(fl!("orchestrator-conversation-sign-in")), None)
        } else if let Some(error) = self.pr.comments_error() {
            (Some(error.to_owned()), None)
        } else if let Some(comments) = self.pr.comments() {
            (None, Some(comments))
        } else {
            let problem = self
                .detail
                .as_ref()
                .and_then(|detail| self.item(&detail.key))
                .and_then(|item| self.lookup_problem(item));
            (
                Some(problem.unwrap_or_else(|| fl!("orchestrator-conversation-reading"))),
                None,
            )
        };
        let open = self
            .open_pull_request()
            .is_some_and(|pr| pr.state == State::Open);
        let chrome = Chrome {
            notice,
            outcome: self.pr.outcome().map(|outcome| outcome.message.clone()),
            failure: self.pr.error().map(str::to_owned),
            composer: (open && self.pr.target().is_some())
                .then(|| self.comment.read(cx).focus.clone()),
        };
        self.conversation.update(comments, chrome);
        self.conversation.hint();
        let list = list(
            self.conversation.list(),
            cx.processor(|this, index: usize, _, cx| this.render_conversation_row(index, cx)),
        )
        .flex_1()
        .min_w_0()
        // A list pads only its top and bottom; each row pads its own sides.
        .py_4();
        self.conversation_scroll_area(list, cx).into_any_element()
    }

    fn render_conversation_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.conversation.get(index) else {
            return div().into_any_element();
        };
        let look = &self.look;
        let content = match &row.piece {
            Piece::Summary(summary) => self.render_summary(summary),
            Piece::Notice(text) => look.muted(text.clone()),
            Piece::Failure(text) => div()
                .text_size(look.small())
                .text_color(rgb(look.hue(RED)))
                .child(text.clone()),
            Piece::Commented(comment) => div()
                .px_3()
                .py(px(6.))
                .bg(rgb(look.theme.surface))
                .child(self.header(comment, &fl!("orchestrator-commented"), None)),
            Piece::Reviewed {
                lead,
                verdict,
                threads,
            } => {
                let action = fl!("orchestrator-reviewed", count = *threads);
                div()
                    .pt(px(6.))
                    .child(self.header(lead, &action, verdict.clone()))
            }
            Piece::Folded { thread, key } => self.render_folded(thread, key.clone(), cx),
            Piece::Thread(thread) => self.render_thread_bar(thread),
            Piece::Body { lines, line } => div()
                .px_3()
                .pt(px(if *line == 0 { 12. } else { 2. }))
                .when(*line + 1 == lines.len(), |el| el.pb_3())
                .min_w_0()
                .child(release_notes::line(
                    "orchestrator-comment-body",
                    *line,
                    &lines[*line],
                    &look.theme,
                    &look.mono,
                )),
            Piece::Empty => div()
                .p_3()
                .child(look.muted(fl!("orchestrator-no-description"))),
            Piece::Composer => self.render_composer(cx),
        };
        let content = self
            .frame(content.flex_1().min_w_0(), row.card)
            .debug_selector(move || format!("orchestrator-conversation-card-{index}"));
        let author = match &row.piece {
            Piece::Commented(comment) | Piece::Reviewed { lead: comment, .. } => Some(comment),
            _ => None,
        };
        let gutter = match (row.gutter, author) {
            (Gutter::Wide, _) => None,
            // The avatar hangs below a row shorter than it, over the blank
            // gutter of the rows under it, so the card's bar keeps its height.
            (Gutter::Avatar, Some(comment)) => Some(
                div().flex_none().w(px(AVATAR)).relative().child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .child(self.avatar(&comment.author, comment.avatar.as_deref())),
                ),
            ),
            _ => Some(div().flex_none().w(px(AVATAR))),
        };
        div()
            .id(("orchestrator-conversation-row", index))
            .debug_selector(move || format!("orchestrator-conversation-row-{index}"))
            .flex()
            .w_full()
            .px_4()
            .gap_3()
            .pt(px(row.gap))
            .children(gutter)
            .child(content)
            .into_any_element()
    }

    /// The share of its card's border a row draws.
    fn frame(&self, content: Div, card: Option<Card>) -> Div {
        let Some(card) = card else {
            return content;
        };
        let theme = &self.look.theme;
        let border = card.tint.map_or(theme.active, |hue| {
            mix(theme.active, self.look.hue(hue), 45)
        });
        content
            .border_l_1()
            .border_r_1()
            .border_color(rgb(border))
            .when(card.top, |el| {
                el.border_t_1().rounded_t(px(corners::CONTROL))
            })
            .when(card.bottom, |el| {
                el.border_b_1().rounded_b(px(corners::CONTROL))
            })
    }

    fn render_summary(&self, summary: &Summary) -> Div {
        let look = &self.look;
        let badges = [
            (
                summary.approved,
                fl!("orchestrator-summary-approved", count = summary.approved),
                GREEN,
            ),
            (
                summary.changes_requested,
                fl!(
                    "orchestrator-summary-changes",
                    count = summary.changes_requested
                ),
                RED,
            ),
            (
                summary.open_threads,
                fl!(
                    "orchestrator-summary-open-threads",
                    count = summary.open_threads
                ),
                YELLOW,
            ),
            (
                summary.resolved_threads,
                fl!(
                    "orchestrator-summary-resolved",
                    count = summary.resolved_threads
                ),
                BLUE,
            ),
        ];
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(corners::CONTROL))
            .bg(rgb(look.theme.surface))
            .text_size(look.small())
            .children(
                badges
                    .into_iter()
                    .filter(|(count, ..)| *count > 0)
                    .map(|(_, text, hue)| look.badge(text, hue)),
            )
            .child(div().flex_1())
            .child(look.muted(fl!(
                "orchestrator-summary-comments",
                comments = summary.comments,
                people = summary.people
            )))
    }

    fn render_folded(&self, thread: &Comment, key: String, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let theme = &look.theme;
        let CommentKind::Thread { path, replies, .. } = &thread.kind else {
            return div();
        };
        let detail = fl!("orchestrator-thread-show", replies = *replies);
        div().child(
            div()
                .id("orchestrator-thread-folded")
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .rounded(px(corners::CONTROL))
                .bg(rgb(theme.surface))
                .text_size(look.small())
                .cursor_pointer()
                .child(div().text_color(rgb(look.hue(GREEN))).child("\u{2713}"))
                .child(
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("orchestrator-thread-resolved")),
                )
                .child(self.path(path))
                .child(div().flex_1())
                .child(look.muted(detail))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.conversation.show(key.clone());
                    cx.notify();
                })),
        )
    }

    fn render_thread_bar(&self, thread: &Comment) -> Div {
        let look = &self.look;
        let CommentKind::Thread {
            path,
            resolved,
            replies,
        } = &thread.kind
        else {
            return div();
        };
        let (state, hue) = if *resolved {
            (fl!("orchestrator-thread-state-resolved"), GREEN)
        } else {
            (fl!("orchestrator-thread-state-open"), YELLOW)
        };
        div()
            .px_3()
            .py_1()
            .bg(rgb(look.theme.surface))
            .flex()
            .items_center()
            .gap_2()
            .child(self.path(path))
            .child(div().flex_1())
            .when(*replies > 0, |el| {
                el.child(look.muted(fl!("orchestrator-replies", count = *replies)))
            })
            .child(look.badge(state, hue))
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> Div {
        let look = &self.look;
        let busy = self.pr.running().map(pr_actions::Action::running_label);
        div()
            .flex()
            .items_center()
            .gap_2()
            .pt_2()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px_2()
                    .py(px(4.))
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(look.theme.active))
                    .child(self.comment.clone()),
            )
            .child(match busy {
                Some(label) => look.muted(label).into_any_element(),
                None => look
                    .button(
                        "orchestrator-comment-send",
                        fl!("orchestrator-comment"),
                        false,
                    )
                    .on_click(cx.listener(|this, _, _, cx| this.post_comment(cx)))
                    .into_any_element(),
            })
    }

    /// The author, a bot tag, what they did, a verdict, and when.
    fn header(&self, comment: &Comment, action: &str, verdict: Option<(String, usize)>) -> Div {
        let look = &self.look;
        div()
            .flex()
            .items_center()
            .gap_2()
            .min_w_0()
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(comment.author.clone()),
            )
            .when(comment.bot, |el| {
                el.child(
                    div()
                        .flex_none()
                        .px(px(5.))
                        .rounded(px(corners::SMALL))
                        .border_1()
                        .border_color(rgb(look.theme.active))
                        .text_size(look.tiny())
                        .text_color(rgb(look.theme.muted))
                        .child(fl!("orchestrator-bot")),
                )
            })
            .child(look.muted(action.to_owned()).min_w_0().truncate())
            .when_some(verdict, |el, (text, hue)| el.child(look.badge(text, hue)))
            .child(div().flex_1())
            .child(
                look.muted(ago(&comment.created_at, chrono::Utc::now()))
                    .flex_none()
                    .text_size(look.small()),
            )
    }

    fn path(&self, path: &str) -> Div {
        let look = &self.look;
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(look.icon("icons/code.svg", 12., look.theme.muted))
            .child(
                look.mono(path.to_owned())
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(look.hue(BLUE))),
            )
    }

    /// The author's GitHub avatar, or their initial on a color of their own
    /// until it arrives or when there is none.
    fn avatar(&self, author: &str, url: Option<&str>) -> AnyElement {
        if let Some(image) = url.and_then(|url| self.avatars.get(url)).cloned().flatten() {
            return img(image)
                .flex_none()
                .size(px(AVATAR))
                .rounded_full()
                .into_any_element();
        }
        initials(&self.look, author)
    }
}

fn initials(look: &Look, author: &str) -> AnyElement {
    let hues = [BLUE, GREEN, YELLOW, RED, MAGENTA, CYAN];
    let pick = author.bytes().map(usize::from).sum::<usize>() % hues.len();
    let fill = mix(look.theme.background, look.hue(hues[pick]), 70);
    div()
        .flex_none()
        .size(px(AVATAR))
        .rounded_full()
        .bg(rgb(fill))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(AVATAR * 0.45))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(look.theme.text_on(fill)))
        .child(
            author
                .chars()
                .next()
                .map_or('?', |first| first.to_ascii_uppercase())
                .to_string(),
        )
        .into_any_element()
}

/// Names a thread across rereads of the conversation.
fn thread_key(comment: &Comment) -> String {
    let path = match &comment.kind {
        CommentKind::Thread { path, .. } => path.as_str(),
        _ => "",
    };
    format!("{}\u{0}{}\u{0}{}", comment.author, path, comment.created_at)
}
