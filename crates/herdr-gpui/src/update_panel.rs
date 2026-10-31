// i18n: migrated
use crate::{
    APP_VERSION, HerdrWindow,
    i18n::fl,
    menu::Page,
    progress::{self, Progress},
    updater::State,
};
use gpui::{prelude::*, *};

/// Lines of notes kept visible however short the window.
const MIN_NOTES_LINES: f32 = 4.;
const MAX_NOTES_LINES: f32 = 14.;
const THUMB_WIDTH: f32 = 4.;
const MIN_THUMB: f32 = 16.;

/// The thumb of the notes' scrollbar: where `max` pixels of overflow, scrolled
/// by `offset` (GPUI's negative scroll offset), sit within `viewport`. `None`
/// when everything fits, so short notes show no bar.
pub(crate) fn scroll_thumb(
    viewport: Bounds<Pixels>,
    offset: Pixels,
    max: Pixels,
) -> Option<Bounds<Pixels>> {
    let (height, max) = (f32::from(viewport.size.height), f32::from(max));
    if max < 1. || height <= 0. {
        return None;
    }
    let thumb = (height * height / (height + max))
        .max(MIN_THUMB)
        .min(height);
    let scrolled = (-f32::from(offset)).clamp(0., max);
    let top = (height - thumb) * scrolled / max;
    Some(Bounds::new(
        point(
            viewport.right() - px(THUMB_WIDTH + 2.),
            viewport.top() + px(top),
        ),
        size(px(THUMB_WIDTH), px(thumb)),
    ))
}

/// GitHub's release notes for the offered update: shown so the changes can be
/// read before installing. Rendered through `release_notes`, so the text is
/// treated as prose and never as instructions — it comes from GitHub, not from
/// the signed manifest.
fn release_notes_section(
    prepared: &crate::release_notes::Prepared,
    scroll: &ScrollHandle,
    notes: &str,
    theme: &crate::config::Theme,
    font: &crate::config::FontConfig,
    mono: &crate::config::FontConfig,
) -> Div {
    // Parsed once per body: scrolling redraws without rebuilding lines.
    let lines = prepared.lines(notes);
    // Short notes keep their own height rather than a blank minimum.
    let min_lines = MIN_NOTES_LINES.min(lines.len() as f32);
    let thumb = rgba((theme.muted << 8) | 0x99);
    let handle = scroll.clone();
    div()
        .debug_selector(|| "app-update-notes".into())
        .flex()
        .flex_col()
        // Shrinks with the window, but never below the heading and the notes'
        // own minimum: a smaller section would let them draw over what follows.
        .min_h(px(font.line_height() * (1. + min_lines) + 4.))
        .gap(px(4.))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child(fl!("update-panel-whats-new")),
        )
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                // In a short window the notes give up height first, so they
                // scroll inside the dialog instead of pushing it into a second,
                // outer scroll that the same wheel would also move.
                .min_h(px(font.line_height() * min_lines))
                .max_h(px(font.line_height() * MAX_NOTES_LINES))
                .child(
                    div()
                        .id("app-update-notes-body")
                        .debug_selector(|| "app-update-notes-body".into())
                        .track_scroll(scroll)
                        .min_h_0()
                        .overflow_y_scroll()
                        // Room for the bar, so it never covers the text.
                        .pr(px(THUMB_WIDTH + 6.))
                        .child(crate::release_notes::render(
                            "app-update-notes",
                            &lines,
                            theme,
                            mono,
                        )),
                )
                // Painted after the notes lay out, so the thumb follows this
                // frame's scroll position rather than the previous one's.
                .child(
                    canvas(
                        |_, _, _| (),
                        move |_, (), window, _| {
                            if let Some(bounds) = scroll_thumb(
                                handle.bounds(),
                                handle.offset().y,
                                handle.max_offset().y,
                            ) {
                                window.paint_quad(
                                    fill(bounds, thumb).corner_radii(px(THUMB_WIDTH / 2.)),
                                );
                            }
                        },
                    )
                    .absolute()
                    .inset_0(),
                ),
        )
}

/// What the taskbar shows for an update: only work the user started, so a
/// background check or a finished download leaves the button alone.
#[cfg(any(windows, test))]
pub(crate) fn taskbar_progress(state: &State) -> Option<Progress> {
    match state {
        State::Downloading { received, total } if *total > 0 => Some(Progress::Fraction(
            (*received as f64 / *total as f64).min(1.) as f32,
        )),
        State::Downloading { .. }
        | State::Installing
        | State::Upgrading { .. }
        | State::Restarting => Some(Progress::Busy),
        _ => None,
    }
}

fn update_progress(state: &State, accent: Hsla, track: Hsla) -> Option<Div> {
    let progress = match state {
        State::Downloading { received, total } if *total > 0 && received < total => {
            Progress::Fraction(*received as f32 / *total as f32)
        }
        State::Ready { .. } | State::Restart { .. } => Progress::Fraction(1.),
        // Homebrew has no reliable overall percentage. A completed archive still
        // needs extraction and verification before it is ready to install.
        State::Checking
        | State::Downloading { .. }
        | State::Installing
        | State::Upgrading { .. }
        | State::Restarting
        | State::Cancelling => Progress::Busy,
        _ => return None,
    };
    Some(progress::bar(
        "app-update-progress",
        progress,
        accent,
        track,
    ))
}

/// What an upgrade is doing: the updater's own steps are messages, and a
/// line the package manager printed is data.
fn step(progress: &crate::updater::Progress) -> String {
    use crate::updater::Progress;
    match progress {
        Progress::Approve => fl!("update-panel-step-approve"),
        Progress::Asking => fl!("update-panel-step-asking"),
        Progress::Checking => fl!("update-panel-step-checking"),
        Progress::Homebrew(line) => fl!("update-panel-step-homebrew", line = line.as_str()),
        Progress::Package(line) => line.clone(),
        Progress::Preparing => fl!("update-panel-step-preparing"),
        Progress::Refreshing => fl!("update-panel-step-refreshing"),
        Progress::Retrying => fl!("update-panel-step-retrying"),
        Progress::Starting => fl!("update-panel-step-starting"),
    }
}

/// Decimal megabytes with one place, as browsers and Finder show downloads.
fn megabytes(bytes: u64) -> String {
    let amount = format!("{:.1}", bytes as f64 / 1_000_000.);
    fl!("update-panel-megabytes", amount = amount)
}

/// The line under a download's bar: how much has arrived, and the percentage
/// when the size is known. `None` outside a download, and once a complete
/// download moves on to verification.
pub(crate) fn download_caption(state: &State) -> Option<(String, Option<String>)> {
    let State::Downloading { received, total } = *state else {
        return None;
    };
    if total == 0 {
        return Some((
            fl!("update-panel-downloaded", received = megabytes(received)),
            None,
        ));
    }
    if received >= total {
        return None;
    }
    Some((
        fl!(
            "update-panel-progress",
            received = megabytes(received),
            total = megabytes(total)
        ),
        Some(fl!(
            "update-panel-percent",
            percent = format!("{:.0}", received as f64 / total as f64 * 100.)
        )),
    ))
}

#[derive(Clone, Copy)]
enum UpdateAction {
    Check,
    Download,
    Install,
    Upgrade,
    Restart,
    Cancel,
}

impl HerdrWindow {
    pub(super) fn open_update_progress_preview(
        &mut self,
        state: State,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::AppUpdate);
        // Every opening starts at the top of the notes.
        self.app_update_notes_scroll.set_offset(Point::default());
        self.update_preview = Some(state);
    }

    pub(super) fn open_app_update(
        &mut self,
        preview: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.page = Some(Page::AppUpdate);
        // Every opening starts at the top of the notes.
        self.app_update_notes_scroll.set_offset(Point::default());
        // The preview exists to exercise the dialog, so it carries a real body:
        // without one the notes section cannot be reviewed at all.
        self.update_preview = preview.then(|| State::Available {
            version: "9999.0.0".into(),
            notes: crate::updater::PREVIEW_NOTES.into(),
        });
    }

    pub(super) fn render_app_update(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let font = &self.config.ui;
        let theme = &self.theme;
        let accent = crate::menu::accent(theme);
        let state = self.update_preview.as_ref().unwrap_or(self.updater.state());
        let (message, action) = match state {
            State::Disabled(reason) => {
                (fl!("update-panel-disabled", reason = reason.as_str()), None)
            }
            State::Idle => (fl!("update-panel-idle"), Some(UpdateAction::Check)),
            State::Checking => (fl!("update-panel-checking"), Some(UpdateAction::Cancel)),
            State::Current => (fl!("update-panel-current"), Some(UpdateAction::Check)),
            State::Available { .. } => {
                (fl!("update-panel-available"), Some(UpdateAction::Download))
            }
            State::Downloading { received, total } => (
                if *total > 0 && received >= total {
                    fl!("update-panel-verifying")
                } else {
                    self.updater.latest_version().map_or_else(
                        || fl!("update-panel-downloading"),
                        |version| fl!("update-panel-downloading-version", version = version),
                    )
                },
                Some(UpdateAction::Cancel),
            ),
            State::Ready { .. } => (
                if cfg!(windows) {
                    fl!("update-panel-ready-windows")
                } else {
                    fl!("update-panel-ready")
                },
                Some(UpdateAction::Install),
            ),
            State::Installing => (fl!("update-panel-installing"), Some(UpdateAction::Cancel)),
            State::Homebrew { .. } => (fl!("update-panel-homebrew"), Some(UpdateAction::Upgrade)),
            State::Upgrading { detail } => (
                step(detail),
                // Homebrew is never interrupted mid-upgrade: see updater::cancel.
                None,
            ),
            State::Restart { version } => (
                fl!("update-panel-restart-needed", version = version.as_str()),
                Some(UpdateAction::Restart),
            ),
            State::Restarting => (fl!("update-panel-restarting"), None),
            State::Cancelling => (fl!("update-panel-cancelling"), None),
            State::Error(error) => (
                fl!("update-panel-failed", error = error.to_string()),
                Some(UpdateAction::Check),
            ),
        };
        let latest = match state {
            State::Available { version, .. }
            | State::Ready { version }
            | State::Homebrew { version, .. }
            | State::Restart { version } => version.clone(),
            State::Current => APP_VERSION.to_owned(),
            _ => self
                .updater
                .latest_version()
                .map_or_else(|| fl!("update-panel-not-known"), str::to_owned),
        };
        // Notes are shown while an update is on offer, not once it is under way.
        let notes = match state {
            State::Available { notes, .. } | State::Homebrew { notes, .. } => Some(notes.as_str()),
            _ => None,
        };
        // The dialog shows a bounded excerpt; the release page has the whole text.
        let notes_page = match state {
            State::Available { version, .. }
            | State::Homebrew { version, .. }
            | State::Ready { version }
            | State::Restart { version } => Some(crate::updater::release_page(version)),
            _ => None,
        };
        let panel = div()
            .debug_selector(|| "app-update-panel".into())
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .max_h((window.viewport_size().height - px(34.)).max(px(0.)))
            .overflow_hidden()
            .child(
                div()
                    .debug_selector(|| "app-update-header".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(12.))
                    .p(px(16.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex_none()
                            .w(px(3.))
                            .h(px(font.size * 2.5))
                            .rounded_full()
                            .bg(accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(fl!("update-panel-title")),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme.muted))
                                    .child(fl!("update-panel-subtitle")),
                            ),
                    )
                    .child(
                        div()
                            .id("app-update-close")
                            .debug_selector(|| "app-update-close".into())
                            .flex_none()
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .rounded(px(crate::config::corners::CONTROL))
                            .text_color(rgb(theme.muted))
                            .hover(|s| s.bg(rgb(theme.active)).text_color(rgb(theme.foreground)))
                            .child(fl!("update-panel-close"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .id("app-update-body")
                    .debug_selector(|| "app-update-body".into())
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p(px(16.))
                    .gap(px(16.))
                    .child(
                        div().flex().flex_col().gap(px(8.)).children(
                            [
                                (
                                    fl!("update-panel-current-version"),
                                    APP_VERSION.to_owned(),
                                    "app-update-current-version",
                                ),
                                (
                                    fl!("update-panel-latest-version"),
                                    latest,
                                    "app-update-latest-version",
                                ),
                            ]
                            .into_iter()
                            .map(|(label, value, selector)| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(16.))
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(px(font.size * 8.))
                                            .text_color(rgb(theme.muted))
                                            .child(label),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || selector.into())
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .child(value),
                                    )
                            }),
                        ),
                    )
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(message))
                    // Only when there is something to read: a release published
                    // without notes leaves the dialog exactly as it was.
                    .when_some(
                        notes.filter(|notes| !notes.trim().is_empty()),
                        |body, notes| {
                            body.child(release_notes_section(
                                &self.app_update_notes,
                                &self.app_update_notes_scroll,
                                notes,
                                theme,
                                font,
                                &self.config.terminal,
                            ))
                        },
                    )
                    .children(
                        update_progress(state, accent.into(), rgb(theme.active).into()).map(
                            |bar| {
                                div().flex().flex_col().gap(px(8.)).child(bar).when_some(
                                    download_caption(state),
                                    |progress, (sizes, percent)| {
                                        progress.child(
                                            div()
                                                .debug_selector(|| {
                                                    "app-update-download-size".into()
                                                })
                                                .flex()
                                                .justify_between()
                                                .gap(px(12.))
                                                .text_size(px(font.size - 1.))
                                                .text_color(rgb(theme.subtext()))
                                                .child(div().min_w_0().truncate().child(sizes))
                                                .children(percent.map(|percent| {
                                                    div()
                                                        .debug_selector(|| {
                                                            "app-update-download-percent".into()
                                                        })
                                                        .flex_none()
                                                        .child(percent)
                                                })),
                                        )
                                    },
                                )
                            },
                        ),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme.subtext()))
                            .child(fl!("update-panel-verified-note")),
                    )
                    .when(self.update_preview.is_some(), |body| {
                        body.child(
                            div()
                                .debug_selector(|| "app-update-preview".into())
                                .p(px(12.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.background))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(fl!("update-panel-preview")),
                                )
                                .child(
                                    div()
                                        .pt(px(4.))
                                        .text_color(rgb(theme.subtext()))
                                        .child(fl!("update-panel-preview-note")),
                                ),
                        )
                    }),
            );
        let mut buttons = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap(px(8.))
            .child(
                div()
                    .id("app-update-releases")
                    .debug_selector(|| "app-update-releases".into())
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .cursor_pointer()
                    .text_color(rgb(theme.muted))
                    .hover(|s| s.bg(rgb(theme.active)).text_color(rgb(theme.foreground)))
                    .child(fl!("update-panel-manual-releases"))
                    .on_click(|_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(crate::updater::RELEASES_PAGE);
                    }),
            )
            .when_some(notes_page, |buttons, url| {
                buttons.child(
                    div()
                        .id("app-update-notes-page")
                        .debug_selector(|| "app-update-notes-page".into())
                        .px(px(12.))
                        .py(px(8.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .cursor_pointer()
                        .text_color(rgb(theme.muted))
                        .hover(|s| s.bg(rgb(theme.active)).text_color(rgb(theme.foreground)))
                        .child(fl!("update-panel-release-notes"))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            cx.open_url(&url);
                        }),
                )
            });
        if let Some(action) = action {
            let label = match action {
                UpdateAction::Check if matches!(state, State::Error(_)) => {
                    fl!("update-panel-retry")
                }
                UpdateAction::Check => fl!("update-panel-check"),
                UpdateAction::Download => fl!("update-panel-download"),
                UpdateAction::Install => fl!("update-panel-install"),
                UpdateAction::Upgrade => fl!("update-panel-upgrade"),
                UpdateAction::Restart => fl!("update-panel-restart"),
                UpdateAction::Cancel => fl!("update-panel-cancel"),
            };
            buttons = buttons.child(
                div()
                    .id("app-update-action")
                    .debug_selector(|| "app-update-action".into())
                    .px(px(12.))
                    .py(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(self.theme.active))
                    .cursor_pointer()
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        // Preview actions must never reach the service, including cancellation.
                        if let Some(state) = this.update_preview.take() {
                            match state {
                                State::Available { version, .. }
                                | State::Homebrew { version, .. } => {
                                    this.update_preview = Some(State::Ready { version });
                                }
                                State::Ready { .. } => this.dismiss_menu(window, cx),
                                _ => this.update_preview = Some(State::Idle),
                            }
                        } else {
                            // A mailbox transition cannot turn a stale Download click into approval.
                            match (action, this.updater.state()) {
                                (
                                    UpdateAction::Check,
                                    State::Idle | State::Current | State::Error(_),
                                ) => this.updater.check(),
                                (UpdateAction::Download, State::Available { .. }) => {
                                    this.updater.download()
                                }
                                (UpdateAction::Install, State::Ready { .. }) => {
                                    this.updater.install()
                                }
                                (UpdateAction::Upgrade, State::Homebrew { .. }) => {
                                    this.updater.upgrade()
                                }
                                (UpdateAction::Restart, State::Restart { .. }) => {
                                    this.updater.restart()
                                }
                                (
                                    UpdateAction::Cancel,
                                    State::Checking | State::Downloading { .. } | State::Installing,
                                ) => this.updater.cancel(),
                                _ => {}
                            }
                        }
                        cx.notify();
                    })),
            );
        }
        panel.child(
            div()
                .debug_selector(|| "app-update-footer".into())
                .flex_none()
                .p(px(16.))
                .border_t_1()
                .border_color(rgb(theme.active))
                .child(buttons),
        )
    }
}

#[cfg(test)]
mod tests;
