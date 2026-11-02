//! Durations and turn summaries a chat shows, written from the typed values
//! herdr-chat-protocol keeps. Its `display` helpers write them in English
//! for herdr-acp's own view; the window writes them in its language.

use crate::i18n::fl;
use herdr_chat_protocol::{TurnEnd, TurnSummary, display};

/// `950ms`, `12s`, `3m 4s`, or `1h 2m`.
pub(crate) fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    match seconds {
        0 => fl!("chat-text-milliseconds", ms = ms),
        1..60 => fl!("chat-text-seconds", seconds = seconds),
        60..3600 => fl!(
            "chat-text-minutes",
            minutes = seconds / 60,
            seconds = seconds % 60
        ),
        _ => fl!(
            "chat-text-hours",
            hours = seconds / 3600,
            minutes = seconds % 3600 / 60
        ),
    }
}

/// `Worked 12s · 3.2k in · 450 out`: how a turn ended, how long it took,
/// and the tokens it used when the agent reports them.
pub(crate) fn turn(summary: &TurnSummary) -> String {
    let duration = duration(summary.duration_ms);
    let ended = match summary.end {
        TurnEnd::Done => fl!("chat-text-turn-done", duration = duration),
        TurnEnd::Cancelled => fl!("chat-text-turn-cancelled", duration = duration),
        TurnEnd::Limit => fl!("chat-text-turn-limit", duration = duration),
        TurnEnd::Refused => fl!("chat-text-turn-refused", duration = duration),
        TurnEnd::Failed => fl!("chat-text-turn-failed", duration = duration),
        TurnEnd::Other => fl!("chat-text-turn-other", duration = duration),
    };
    let Some(tokens) = &summary.tokens else {
        return ended;
    };
    // Token counts in the protocol's short form: `950`, `18.6k`, `1.2M`.
    let (input, output) = (
        display::tokens(tokens.input),
        display::tokens(tokens.output),
    );
    if tokens.thought > 0 {
        fl!(
            "chat-text-turn-tokens-thinking",
            turn = ended,
            input = input,
            output = output,
            thought = display::tokens(tokens.thought)
        )
    } else {
        fl!(
            "chat-text-turn-tokens",
            turn = ended,
            input = input,
            output = output
        )
    }
}

#[cfg(test)]
mod tests;
