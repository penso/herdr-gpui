//! When a chat change starts, updates, or ends the chat's Live Activity, and
//! what it shows. Pure, with the inputs [`crate::push::decide`] takes: the
//! daemon passes each change with the transcript it applies to, before
//! applying it, and the time.
//!
//! | Change | Activity |
//! | --- | --- |
//! | a turn starts | start, working |
//! | a tool call starts or the plan moves, during a turn | update with the step |
//! | a permission is asked (not auto-accepted) | update, question, alert |
//! | the last waiting permission is resolved | update, working |
//! | the turn's summary arrives | end, finished (failed when it failed) |
//! | the turn ends without a summary, or the agent fails mid-turn | end, failed |
//!
//! A cancelled turn ends at once, since the user stopped it; any other ends
//! and stays on the lock screen for [`LINGER_S`].

use super::{Alert, Attributes, ContentState, Event, Live, Phase};
use crate::push::{Chat, excerpt};
use herdr_chat_protocol::{
    Change, EntryKind, PlanEntry, PlanStatus, Status, Tool, ToolStatus, Transcript, TurnEnd,
    TurnSummary, display,
};

/// How long a finished activity stays on the lock screen.
pub(crate) const LINGER_S: u64 = 15 * 60;
/// The step shown before the agent names one.
pub(crate) const DEFAULT_STEP: &str = "Working";

/// The push `change` calls for, given the transcript `before` it applies to,
/// on the computer named `computer` whose CA has the SHA-256 `fingerprint`
/// (any case, colons allowed, as `state.fingerprint` writes it), at
/// `now_ms`.
pub(crate) fn decide(
    chat: &Chat<'_>,
    computer: &str,
    fingerprint: &str,
    before: &Transcript,
    change: &Change,
    now_ms: u64,
) -> Option<Live> {
    let now_s = now_ms / 1000;
    let running = before.turn.is_some();
    match change {
        Change::Turn { turn: Some(turn) } if !running => {
            let mut state = state(before, Phase::Working, DEFAULT_STEP.to_owned());
            state.started_at = turn.started_ms / 1000;
            // The plan and its counts belong to the turn before.
            state.plan_done = None;
            state.plan_total = None;
            Some(live(
                chat,
                Event::Start {
                    alert: Alert {
                        body: place(chat),
                        title: format!("{} is working", chat.agent),
                    },
                    attributes: Attributes {
                        agent: excerpt(chat.agent),
                        chat_title: excerpt(chat.title),
                        computer: excerpt(computer),
                        computer_id: computer_id(fingerprint),
                        pane: chat.pane.to_owned(),
                        workspace: excerpt(chat.workspace),
                    },
                },
                state,
            ))
        }
        Change::Turn { turn: None } if running && !summarized(before) => {
            let mut state = state(before, Phase::Failed, current_step(before));
            state.summary = Some(format!("{} stopped", chat.agent));
            Some(end(chat, state, now_s + LINGER_S))
        }
        Change::Push { entry } if running => match &entry.kind {
            EntryKind::Turn(summary) => Some(turn_ended(chat, before, summary, now_s)),
            EntryKind::Tool(tool) => tool_step(chat, before, tool),
            _ => None,
        },
        Change::Replace { entry } if running => match &entry.kind {
            EntryKind::Tool(tool) => tool_step(chat, before, tool),
            _ => None,
        },
        Change::Plan { plan } if running => {
            let mut state = state(before, phase(before), step(before, plan));
            (state.plan_done, state.plan_total) = plan_counts(plan);
            let unchanged = state.step == current_step(before)
                && (state.plan_done, state.plan_total) == plan_counts(&before.plan);
            (!unchanged).then(|| update(chat, state, None))
        }
        Change::PermissionAsked { permission } if running && !before.auto_accept => {
            let mut state = state(before, Phase::Question, current_step(before));
            state.question = Some(excerpt(&permission.title));
            let alert = Alert {
                body: place(chat),
                title: format!("{} needs an answer", chat.agent),
            };
            Some(update(chat, state, Some(alert)))
        }
        Change::PermissionResolved { id } if running => {
            let last = matches!(before.permissions.as_slice(), [only] if only.id == *id);
            last.then(|| {
                update(
                    chat,
                    state(before, Phase::Working, current_step(before)),
                    None,
                )
            })
        }
        Change::Status {
            status: Status::Failed { message },
        } if running => {
            let mut state = state(before, Phase::Failed, current_step(before));
            state.summary = Some(excerpt(message));
            Some(end(chat, state, now_s + LINGER_S))
        }
        _ => None,
    }
}

fn turn_ended(chat: &Chat<'_>, before: &Transcript, summary: &TurnSummary, now_s: u64) -> Live {
    let phase = match summary.end {
        TurnEnd::Failed | TurnEnd::Refused => Phase::Failed,
        TurnEnd::Done | TurnEnd::Cancelled | TurnEnd::Limit | TurnEnd::Other => Phase::Finished,
    };
    let mut state = state(before, phase, current_step(before));
    let mut facts = summary.describe();
    if let Some(cost) = &state.cost {
        facts.push_str(" \u{b7} ");
        facts.push_str(cost);
    }
    state.summary = Some(facts);
    let dismissal_s = if summary.end == TurnEnd::Cancelled {
        now_s
    } else {
        now_s + LINGER_S
    };
    end(chat, state, dismissal_s)
}

/// An update naming `tool` as the step, when it is running and is not
/// already the step shown.
fn tool_step(chat: &Chat<'_>, before: &Transcript, tool: &Tool) -> Option<Live> {
    let running = matches!(tool.status, ToolStatus::Pending | ToolStatus::InProgress);
    if !running || tool.title.trim().is_empty() {
        return None;
    }
    let step = excerpt(&tool.title);
    if step == current_step(before) {
        return None;
    }
    Some(update(chat, state(before, phase(before), step), None))
}

/// The phase a turn is in apart from the change at hand.
fn phase(before: &Transcript) -> Phase {
    if before.is_blocked() {
        Phase::Question
    } else {
        Phase::Working
    }
}

/// The content shared by every push for a running turn.
fn state(before: &Transcript, phase: Phase, step: String) -> ContentState {
    let (plan_done, plan_total) = plan_counts(&before.plan);
    ContentState {
        cost: before
            .usage
            .as_ref()
            .and_then(|usage| usage.cost.as_ref())
            .map(display::cost),
        phase,
        plan_done,
        plan_total,
        question: (phase == Phase::Question)
            .then(|| before.permissions.first())
            .flatten()
            .map(|permission| excerpt(&permission.title)),
        started_at: before.turn.map_or(0, |turn| turn.started_ms / 1000),
        step,
        summary: None,
        waiting: 0,
        working: 0,
    }
}

fn plan_counts(plan: &[PlanEntry]) -> (Option<u32>, Option<u32>) {
    if plan.is_empty() {
        return (None, None);
    }
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let done = plan
        .iter()
        .filter(|entry| entry.status == PlanStatus::Completed)
        .count();
    (Some(count(done)), Some(count(plan.len())))
}

/// What the turn is doing now: the running tool call of the current turn,
/// else the plan's step in progress, else the turn's last tool call.
pub(crate) fn current_step(transcript: &Transcript) -> String {
    step(transcript, &transcript.plan)
}

/// [`current_step`] of `transcript`'s entries with `plan` as its plan.
fn step(transcript: &Transcript, plan: &[PlanEntry]) -> String {
    let turn = transcript
        .entries
        .iter()
        .rev()
        .take_while(|entry| !matches!(entry.kind, EntryKind::User { .. }));
    let mut last_tool = None;
    for entry in turn {
        let EntryKind::Tool(tool) = &entry.kind else {
            continue;
        };
        if tool.title.trim().is_empty() {
            continue;
        }
        if matches!(tool.status, ToolStatus::Pending | ToolStatus::InProgress) {
            return excerpt(&tool.title);
        }
        last_tool.get_or_insert(&tool.title);
    }
    let planned = plan
        .iter()
        .find(|entry| entry.status == PlanStatus::InProgress)
        .map(|entry| entry.content.as_str());
    planned
        .or(last_tool.map(String::as_str))
        .map_or_else(|| DEFAULT_STEP.to_owned(), excerpt)
}

/// Whether the transcript's last entry is a turn summary, so a following
/// turn end needs no push of its own.
fn summarized(transcript: &Transcript) -> bool {
    transcript
        .entries
        .last()
        .is_some_and(|entry| matches!(entry.kind, EntryKind::Turn(_)))
}

/// The activity's `computerId`: the CA fingerprint as 64 uppercase hex
/// digits without separators, the form the pairing QR carries and the app
/// stores (`Fingerprint.hex`).
pub(crate) fn computer_id(fingerprint: &str) -> String {
    fingerprint
        .chars()
        .filter(|c| *c != ':')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

fn place(chat: &Chat<'_>) -> String {
    if chat.title.is_empty() {
        excerpt(chat.workspace)
    } else {
        excerpt(&format!("{} \u{b7} {}", chat.workspace, chat.title))
    }
}

fn live(chat: &Chat<'_>, event: Event, state: ContentState) -> Live {
    Live {
        event,
        pane: chat.pane.to_owned(),
        state,
    }
}

fn update(chat: &Chat<'_>, state: ContentState, alert: Option<Alert>) -> Live {
    live(chat, Event::Update { alert }, state)
}

fn end(chat: &Chat<'_>, state: ContentState, dismissal_s: u64) -> Live {
    live(chat, Event::End { dismissal_s }, state)
}
