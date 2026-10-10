//! A pull request's conversation, drawn the way GitHub reads: each comment a
//! card under its author's avatar, each review holding the inline threads
//! its author left, resolved threads folded to a line, and a summary of the
//! verdicts on top. Bodies are Markdown, parsed once per text; avatars are
//! read off the UI thread and only ever from GitHub's avatar host.

use super::{
    OrchestratorView,
    look::{BLUE, CYAN, GREEN, Look, MAGENTA, RED, YELLOW},
};
use crate::{
    config::{corners, mix},
    pr_actions::{self, Comment, CommentKind, Review},
    pull_request::State,
    release_notes::{self, Line},
};
use gpui::{prelude::*, *};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

const AVATAR: f32 = 32.;
/// Avatars kept per view; a conversation holds at most a few dozen authors.
const AVATAR_LIMIT: usize = 128;
/// Parsed bodies kept per view before the cache starts over.
const BODY_LIMIT: usize = 128;
/// Resolved threads unfolded per view before they all fold again.
const SHOWN_LIMIT: usize = 256;
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
#[derive(Debug, Default, PartialEq, Eq)]
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

/// "2h ago" from an RFC 3339 time, or nothing when it does not parse.
pub(super) fn ago(at: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let at = chrono::DateTime::parse_from_rfc3339(at)
        .ok()
        .map(|at| at.with_timezone(&chrono::Utc));
    match super::look::age(at, now) {
        age if age.is_empty() || age == "now" => age,
        age => format!("{age} ago"),
    }
}

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// Parsed comment bodies, so a repaint reuses them.
#[derive(Default)]
pub(super) struct Bodies(RefCell<HashMap<String, Rc<[Line]>>>);

impl Bodies {
    fn lines(&self, body: &str) -> Rc<[Line]> {
        let mut cached = self.0.borrow_mut();
        if let Some(lines) = cached.get(body) {
            return lines.clone();
        }
        if cached.len() >= BODY_LIMIT {
            cached.clear();
        }
        let lines: Rc<[Line]> = release_notes::parse(&super::markdown::without_html(body)).into();
        cached.insert(body.to_owned(), lines.clone());
        lines
    }
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

    pub(super) fn render_conversation(&self, cx: &mut Context<Self>) -> AnyElement {
        let look = &self.look;
        let mut column = div()
            .id("orchestrator-conversation")
            .flex_1()
            .min_w_0()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_3();
        if self.request.token.is_none() {
            column = column.child(look.muted("Sign in to GitHub to read the conversation."));
        } else if let Some(error) = self.pr.comments_error() {
            column = column.child(look.muted(error.to_owned()));
        } else if let Some(comments) = self.pr.comments() {
            if comments.is_empty() {
                column = column.child(look.muted("No comments yet."));
            } else {
                column = column.child(self.render_summary(&summary(comments)));
            }
            let now = chrono::Utc::now();
            for (index, entry) in entries(comments).into_iter().enumerate() {
                column = column.child(match entry {
                    Entry::Comment(comment) => self.render_comment(index, comment, now),
                    Entry::Review {
                        author,
                        review,
                        threads,
                    } => self.render_review(index, author, review, &threads, now, cx),
                });
            }
        } else if let Some(problem) = self
            .detail
            .as_ref()
            .and_then(|detail| self.item(&detail.key))
            .and_then(|item| self.lookup_problem(item))
        {
            column = column.child(look.muted(problem));
        } else {
            column = column.child(look.muted("Reading the conversation\u{2026}"));
        }
        if let Some(outcome) = self.pr.outcome() {
            column = column.child(look.muted(outcome.message.clone()));
        }
        if let Some(error) = self.pr.error() {
            column = column.child(
                div()
                    .text_size(look.small())
                    .text_color(rgb(look.hue(RED)))
                    .child(error.to_owned()),
            );
        }
        let open = self
            .open_pull_request()
            .is_some_and(|pr| pr.state == State::Open);
        if open && self.pr.target().is_some() {
            column = column.child(self.render_composer(cx));
        }
        column.into_any_element()
    }

    fn render_summary(&self, summary: &Summary) -> Div {
        let look = &self.look;
        let badges = [
            (summary.approved, "approved", "approved", GREEN),
            (
                summary.changes_requested,
                "requested changes",
                "requested changes",
                RED,
            ),
            (summary.open_threads, "open thread", "open threads", YELLOW),
            (summary.resolved_threads, "resolved", "resolved", BLUE),
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
                    .map(|(count, one, many, hue)| look.badge(plural(count, one, many), hue)),
            )
            .child(div().flex_1())
            .child(look.muted(format!(
                "{} from {}",
                plural(summary.comments, "comment", "comments"),
                plural(summary.people, "person", "people")
            )))
    }

    fn render_comment(
        &self,
        index: usize,
        comment: &Comment,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        div()
            .id(("orchestrator-comment", index))
            .flex()
            .gap_3()
            .child(self.avatar(&comment.author, comment.avatar.as_deref()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(corners::CONTROL))
                    .border_1()
                    .border_color(rgb(theme.active))
                    .overflow_hidden()
                    .child(
                        div()
                            .px_3()
                            .py(px(6.))
                            .bg(rgb(theme.surface))
                            .child(self.header(comment, "commented", None, now)),
                    )
                    .child(self.body(comment)),
            )
            .into_any_element()
    }

    fn render_review(
        &self,
        index: usize,
        author: &str,
        review: Option<&Comment>,
        threads: &[&Comment],
        now: chrono::DateTime<chrono::Utc>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let lead = review.or(threads.first().copied());
        let verdict = review.and_then(|review| match review.kind {
            CommentKind::Review(Review::Approved) => Some(("approved", GREEN)),
            CommentKind::Review(Review::ChangesRequested) => Some(("changes requested", RED)),
            CommentKind::Review(Review::Dismissed) => Some(("dismissed", MAGENTA)),
            _ => None,
        });
        let action = if threads.is_empty() {
            "reviewed".to_owned()
        } else {
            format!(
                "reviewed \u{00b7} {}",
                plural(threads.len(), "comment", "comments")
            )
        };
        let header = match lead {
            Some(lead) => self.header(lead, &action, verdict, now),
            None => div().child(author.to_owned()),
        };
        let body = review.filter(|review| !review.source.trim().is_empty());
        let tint = verdict.map_or(theme.active, |(_, hue)| {
            mix(theme.active, look.hue(hue), 45)
        });
        div()
            .id(("orchestrator-review", index))
            .flex()
            .gap_3()
            .child(self.avatar(author, lead.and_then(|lead| lead.avatar.as_deref())))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().pt(px(6.)).child(header))
                    .when_some(body, |el, review| {
                        el.child(
                            div()
                                .rounded(px(corners::CONTROL))
                                .border_1()
                                .border_color(rgb(tint))
                                .child(self.body(review)),
                        )
                    })
                    .children(
                        threads.iter().enumerate().map(|(thread, comment)| {
                            self.render_thread(index, thread, comment, cx)
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_thread(
        &self,
        review: usize,
        index: usize,
        comment: &Comment,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &self.look;
        let theme = &look.theme;
        let CommentKind::Thread {
            path,
            resolved,
            replies,
        } = &comment.kind
        else {
            return div().into_any_element();
        };
        let key = thread_key(comment);
        let id = SharedString::from(format!("orchestrator-thread-{review}-{index}"));
        if *resolved && !self.shown_threads.contains(&key) {
            let mut detail = String::new();
            if *replies > 0 {
                detail = format!(
                    "{} \u{00b7} ",
                    plural(
                        usize::try_from(*replies).unwrap_or(usize::MAX),
                        "reply",
                        "replies"
                    )
                );
            }
            detail.push_str("Show");
            return div()
                .id(id)
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
                .child(div().text_color(rgb(theme.subtext())).child("Resolved"))
                .child(self.path(path))
                .child(div().flex_1())
                .child(look.muted(detail))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.shown_threads.len() >= SHOWN_LIMIT {
                        this.shown_threads.clear();
                    }
                    this.shown_threads.insert(key.clone());
                    cx.notify();
                }))
                .into_any_element();
        }
        let (state, hue) = if *resolved {
            ("resolved", GREEN)
        } else {
            ("open", YELLOW)
        };
        div()
            .id(id)
            .rounded(px(corners::CONTROL))
            .border_1()
            .border_color(rgb(theme.active))
            .overflow_hidden()
            .child(
                div()
                    .px_3()
                    .py_1()
                    .bg(rgb(theme.surface))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.path(path))
                    .child(div().flex_1())
                    .when(*replies > 0, |el| {
                        el.child(look.muted(plural(
                            usize::try_from(*replies).unwrap_or(usize::MAX),
                            "reply",
                            "replies",
                        )))
                    })
                    .child(look.badge(state, hue)),
            )
            .child(self.body(comment))
            .into_any_element()
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
                    .button("orchestrator-comment-send", "Comment", false)
                    .on_click(cx.listener(|this, _, _, cx| this.post_comment(cx)))
                    .into_any_element(),
            })
    }

    /// The author, a bot tag, what they did, a verdict, and when.
    fn header(
        &self,
        comment: &Comment,
        action: &str,
        verdict: Option<(&'static str, usize)>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Div {
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
                        .child("bot"),
                )
            })
            .child(look.muted(action.to_owned()).min_w_0().truncate())
            .when_some(verdict, |el, (text, hue)| el.child(look.badge(text, hue)))
            .child(div().flex_1())
            .child(
                look.muted(ago(&comment.created_at, now))
                    .flex_none()
                    .text_size(look.small()),
            )
    }

    fn body(&self, comment: &Comment) -> Div {
        let look = &self.look;
        if comment.source.trim().is_empty() {
            return div().p_3().child(look.muted("No description provided."));
        }
        let lines = self.bodies.lines(&comment.source);
        div().p_3().min_w_0().child(release_notes::render(
            "orchestrator-comment-body",
            &lines,
            &look.theme,
            &look.mono,
        ))
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
