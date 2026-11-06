//! Short text for numbers both clients show the same way: durations, token
//! counts, costs, and how a turn ended.

use crate::{Cost, TurnEnd, TurnSummary};

/// `950ms`, `12s`, `3m 4s`, or `1h 2m`.
pub fn duration(ms: u64) -> String {
    let seconds = ms / 1000;
    match seconds {
        0 => format!("{ms}ms"),
        1..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
    }
}

/// `950`, `18.6k`, or `1.2M` tokens.
pub fn tokens(count: u64) -> String {
    match count {
        0..1000 => count.to_string(),
        1000..1_000_000 => trim(format!("{:.1}", count as f64 / 1000.0)) + "k",
        _ => trim(format!("{:.1}", count as f64 / 1_000_000.0)) + "M",
    }
}

fn trim(number: String) -> String {
    number
        .strip_suffix(".0")
        .map_or(number.clone(), str::to_owned)
}

/// `$0.42`, or `0.42 EUR` for a currency other than dollars.
pub fn cost(cost: &Cost) -> String {
    let amount = cost.micros as f64 / 1_000_000.0;
    let digits = if amount < 0.01 && cost.micros > 0 {
        4
    } else {
        2
    };
    match cost.currency.as_str() {
        "USD" | "usd" | "" => format!("${amount:.digits$}"),
        currency => format!("{amount:.digits$} {currency}"),
    }
}

impl TurnSummary {
    /// `Worked 12s · 3.2k in · 450 out`.
    pub fn describe(&self) -> String {
        let verb = match self.end {
            TurnEnd::Done => "Worked",
            TurnEnd::Cancelled => "Cancelled after",
            TurnEnd::Limit => "Stopped at its limit after",
            TurnEnd::Refused => "Refused after",
            TurnEnd::Failed => "Failed after",
            TurnEnd::Other => "Ended after",
        };
        let mut text = format!("{verb} {}", duration(self.duration_ms));
        if let Some(tokens) = &self.tokens {
            text.push_str(&format!(
                " \u{b7} {} in \u{b7} {} out",
                self::tokens(tokens.input),
                self::tokens(tokens.output)
            ));
            if tokens.thought > 0 {
                text.push_str(&format!(
                    " \u{b7} {} thinking",
                    self::tokens(tokens.thought)
                ));
            }
        }
        text
    }
}
