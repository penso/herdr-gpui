//! What a pull request row's cells say: its review, where it stands, and
//! its checks.

use super::look::{BLUE, GREEN, Look, MAGENTA, RED, YELLOW};
use crate::{
    i18n::fl,
    orchestrator::{CheckState, Item, PrStatus, Provider, Review},
    pull_request::{MergeState, Tone},
};
use gpui::{prelude::*, *};

fn check_hue(look: &Look, state: CheckState) -> u32 {
    match state {
        CheckState::Failed => look.hue(RED),
        CheckState::Pending => look.hue(YELLOW),
        CheckState::Passed => look.hue(GREEN),
        CheckState::Skipped => look.theme.muted,
    }
}

/// The review cell's words and hue: a review asked of the reader, else the
/// forge's verdict.
pub(super) fn review_text(
    status: Option<&PrStatus>,
    login: Option<&str>,
) -> Option<(String, usize)> {
    let status = status?;
    if login.is_some_and(|login| status.requests(login)) {
        return Some((fl!("orchestrator-review-yours"), MAGENTA));
    }
    Some(match status.review? {
        Review::Approved => (fl!("orchestrator-review-approved"), GREEN),
        Review::ApprovedWithSuggestions => (fl!("orchestrator-review-suggestions"), GREEN),
        Review::ChangesRequested => (fl!("orchestrator-review-changes"), RED),
        Review::Rejected => (fl!("orchestrator-review-rejected"), RED),
        Review::Required => (fl!("orchestrator-review-required"), BLUE),
        Review::WaitingForAuthor => (fl!("orchestrator-review-waiting"), YELLOW),
    })
}

pub(super) fn review_dot(look: &Look, status: Option<&PrStatus>) -> Option<Div> {
    let hue = match status?.review? {
        Review::Approved | Review::ApprovedWithSuggestions => GREEN,
        Review::ChangesRequested | Review::Rejected => RED,
        Review::WaitingForAuthor => YELLOW,
        Review::Required => BLUE,
    };
    Some(look.dot(look.hue(hue), 7.))
}

pub(super) fn checks_dot(look: &Look, status: Option<&PrStatus>) -> Option<Div> {
    let state = status?.checks_state()?;
    Some(look.dot(check_hue(look, state), 7.))
}

/// The tone of a row's pull request, the one its sidebar badge takes. One
/// with no status yet, or a merge request, says nothing rather than ready;
/// so does an open one on Azure DevOps, which reports neither whether it can
/// merge nor its checks here, only the verdict the review column shows.
pub(super) fn row_tone(item: &Item, status: Option<&PrStatus>) -> Tone {
    match item.state.as_str() {
        "merged" => Tone::Merged,
        "closed" => Tone::Attention,
        "draft" => Tone::Muted,
        _ if item.key.source.provider == Provider::Azure => Tone::Muted,
        _ => status.map_or(Tone::Muted, PrStatus::tone),
    }
}

/// The word beside the tone's color, naming what no other column shows:
/// conflicts and a branch behind its base. A draft's badge is by its title.
pub(super) fn standing_word(tone: Tone, item: &Item, status: Option<&PrStatus>) -> Option<String> {
    let merge = status.map_or(MergeState::Unknown, |status| status.merge);
    Some(match tone {
        Tone::Attention if item.state == "closed" => {
            if item.key.source.provider == Provider::Azure {
                fl!("orchestrator-standing-abandoned")
            } else {
                fl!("orchestrator-standing-closed")
            }
        }
        Tone::Attention if merge == MergeState::Dirty => fl!("orchestrator-standing-conflicts"),
        Tone::Attention => fl!("orchestrator-standing-attention"),
        Tone::Held if merge == MergeState::Behind => fl!("orchestrator-standing-behind"),
        Tone::Held => fl!("orchestrator-badge-blocked"),
        Tone::Merged => fl!("orchestrator-standing-merged"),
        Tone::Muted => return None,
        Tone::Ready => fl!("orchestrator-standing-ready"),
        Tone::Waiting => fl!("orchestrator-standing-waiting"),
    })
}

/// How many checks are fine out of how many reported, in the color and mark
/// of what they add up to.
pub(super) fn checks_count(look: &Look, status: Option<&PrStatus>) -> Option<Div> {
    let status = status.filter(|status| !status.checks.is_empty())?;
    let state = status.checks_state().unwrap_or(CheckState::Skipped);
    let icon = match state {
        CheckState::Failed => "icons/error.svg",
        CheckState::Pending => "icons/refresh.svg",
        CheckState::Passed => "icons/check.svg",
        CheckState::Skipped => "icons/pass.svg",
    };
    let hue = check_hue(look, state);
    Some(
        div()
            .flex()
            .items_center()
            .gap_1()
            .child(look.icon(icon, 11., hue))
            .child(
                look.mono(format!("{}/{}", status.checks_ok(), status.checks.len()))
                    .text_size(look.small())
                    .text_color(rgb(hue)),
            ),
    )
}
