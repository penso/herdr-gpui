//! Shared fixtures; each topic lives in its own file under `tests/`.

use super::*;
use std::convert::Infallible;

mod budgets;
mod declare;
mod next;
mod plan;
mod sandbox;
mod schedule;
mod steps;
mod tokens;
mod walker;

const CAPS: &str =
    "herdr.caps { agents_per_run = 2, tokens_per_day = 2_000_000, wall_time = \"3h\" }\n";

/// `body` after a valid `herdr.caps` line.
fn source(body: &str) -> Source {
    Source::new("test", format!("{CAPS}{body}"))
}

/// A fresh state with `body` loaded.
fn load(body: &str) -> Result<Script> {
    source(body).load(Limits::default())
}

/// The refusal a load or run stopped with.
fn refusal<T: std::fmt::Debug>(result: Result<T>) -> Refusal {
    result.unwrap_err().refusal().cloned().unwrap()
}

/// Runs trigger `index` of `body` in a fresh state, every step succeeding.
fn record(body: &str, index: usize) -> (Vec<Call>, End<Infallible>) {
    let mut calls = Vec::new();
    let end = load(body)
        .unwrap()
        .run(index, |call| {
            calls.push(call.clone());
            Verdict::Succeeded
        })
        .unwrap();
    (calls, end)
}

/// The plan of `body`.
fn plan(body: &str) -> Plan {
    Plan::build(&source(body), Limits::default()).unwrap()
}
