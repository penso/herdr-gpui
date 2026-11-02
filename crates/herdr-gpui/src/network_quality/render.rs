//! `NET 31ms 0%` in the status bar, after CPU and memory, with the recent
//! probes summarized in a tooltip.

use super::{INTERVAL, LATENCY_PROBES, Reading, TARGET, WINDOW, probe::Outcome};
use crate::{config::Theme, i18n::fl, window::HerdrWindow};
use gpui::{prelude::*, *};
use std::time::Duration;

/// Round trips this slow make typing in a remote shell feel late.
const LATENCY_WARN_MS: f32 = 150.;
const LATENCY_CRITICAL_MS: f32 = 400.;
/// Any steady loss stalls TCP; this much makes connections drop.
const LOSS_WARN: f32 = 5.;
const LOSS_CRITICAL: f32 = 15.;

impl HerdrWindow {
    /// Nothing while hidden or before the first probe ends.
    pub(crate) fn render_network_quality(&self, cx: &App) -> Option<impl IntoElement> {
        if !self.config.show_network_quality {
            return None;
        }
        let reading = super::reading(cx)?;
        reading.latest()?;
        let theme = &self.theme;
        let colored =
            |text: String, color: u32| div().flex_none().text_color(rgb(color)).child(text);
        let latency = match reading.latency() {
            Some(latency) => {
                let milliseconds = millis(latency);
                colored(
                    fl!("network-quality-latency", ms = format!("{milliseconds:.0}")),
                    graded(milliseconds, LATENCY_WARN_MS, LATENCY_CRITICAL_MS, theme),
                )
            }
            None => colored(fl!("network-quality-offline"), theme.ink(theme.palette[1])),
        };
        let spoken = match (reading.latency(), reading.loss_percent()) {
            (Some(latency), Some(loss)) => fl!(
                "network-quality-spoken-loss",
                ms = format!("{:.0}", millis(latency)),
                loss = format!("{loss:.0}")
            ),
            (Some(latency), None) => fl!(
                "network-quality-spoken",
                ms = format!("{:.0}", millis(latency))
            ),
            (None, _) => fl!("network-quality-spoken-offline"),
        };
        let loss = reading.loss_percent().map(|loss| {
            colored(
                format!("{loss:.0}%"),
                graded(loss, LOSS_WARN, LOSS_CRITICAL, theme),
            )
        });
        Some(
            div()
                .id("network-quality")
                .debug_selector(|| "network-quality".into())
                .role(Role::Label)
                .aria_value(spoken)
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.))
                // CPU and memory's 6px padding and the bar's 6px gap
                // already sit on the left, and the bar's gap and the next
                // button's 8px padding on the right; these margins bring both
                // sides to the 10px between CPU and MEM.
                .ml(px(-2.))
                .mr(px(-4.))
                .h_full()
                .tooltip(tooltip(reading, theme))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.muted))
                        .child(fl!("network-quality-label")),
                )
                .child(latency)
                .children(loss),
        )
    }
}

fn millis(duration: Duration) -> f32 {
    duration.as_secs_f32() * 1000.
}

/// Foreground below `warn`, yellow from it, red from `critical`, matching
/// the CPU and memory colors.
fn graded(value: f32, warn: f32, critical: f32, theme: &Theme) -> u32 {
    if value >= critical {
        theme.ink(theme.palette[1])
    } else if value >= warn {
        theme.ink(theme.palette[3])
    } else {
        theme.foreground
    }
}

fn tooltip(
    reading: &Reading,
    theme: &Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text = SharedString::from(details(reading));
    let (foreground, surface) = (theme.foreground, theme.surface);
    move |_, cx| {
        cx.new(|_| crate::usage::Hint {
            text: text.clone(),
            foreground,
            surface,
        })
        .into()
    }
}

pub(super) fn details(reading: &Reading) -> String {
    let window_minutes = WINDOW as u64 * INTERVAL.as_secs() / 60;
    let ms = |duration: Duration| format!("{:.0}", millis(duration));
    let mut lines = vec![fl!(
        "network-quality-target",
        target = TARGET.to_string(),
        seconds = INTERVAL.as_secs()
    )];
    match reading.latest() {
        Some(Outcome::Reply(round_trip)) => {
            lines.push(fl!("network-quality-last", ms = ms(round_trip)));
        }
        // The system's words for why the connection failed.
        Some(Outcome::Lost(kind)) => {
            lines.push(fl!("network-quality-last-lost", reason = kind.to_string()))
        }
        None => {}
    }
    if let Some(latency) = reading.latency() {
        lines.push(fl!(
            "network-quality-median",
            ms = ms(latency),
            probes = LATENCY_PROBES
        ));
    }
    if let Some((min, max)) = reading.range() {
        lines.push(fl!(
            "network-quality-range",
            min = ms(min),
            max = ms(max),
            minutes = window_minutes
        ));
    }
    if let Some(loss) = reading.loss_percent() {
        lines.push(fl!(
            "network-quality-loss",
            loss = format!("{loss:.0}"),
            probes = reading.probes(),
            minutes = window_minutes
        ));
    }
    lines.join("\n")
}
