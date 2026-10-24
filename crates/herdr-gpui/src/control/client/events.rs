//! `events` and `wait`.
use super::{Context, Output, unexpected};
use crate::{
    cli::control::{Cursor, EventKind, WaitFor},
    control::protocol::{
        Request, Response,
        events::{Condition, Event, EventsOp, Reply},
    },
};
use std::io::Write as _;

/// How long one request of `events --follow` waits before asking again.
const FOLLOW_WAIT: u64 = 300;

/// One event as text, starting with the cursor to resume after it.
pub(super) fn line(event: &Event, instance: Option<u64>) -> String {
    let detail = event
        .detail
        .as_deref()
        .map(|detail| format!(" {detail}"))
        .unwrap_or_default();
    super::printable(
        &format!(
            "{} {} {}{detail}",
            Cursor {
                instance,
                seq: event.seq
            },
            event.kind.name(),
            event.subject
        ),
        4096,
    )
}

fn poll(
    after: Option<Cursor>,
    wait: u64,
    kinds: &[EventKind],
    context: &Context,
) -> crate::Result<(Vec<Event>, Cursor)> {
    match context.call(
        Request::Events,
        EventsOp::Poll {
            after: after.map(|cursor| cursor.seq),
            instance: after.and_then(|cursor| cursor.instance),
            wait_seconds: wait,
            kinds: kinds.to_vec(),
        },
    )? {
        Response::Events {
            reply:
                Reply::Events {
                    events,
                    next,
                    instance,
                },
        } => Ok((
            events,
            Cursor {
                instance: Some(instance),
                seq: next,
            },
        )),
        response => Err(unexpected(response)),
    }
}

pub(super) fn events(
    after: Option<Cursor>,
    follow: bool,
    wait: u64,
    kinds: Vec<EventKind>,
    context: &Context,
) -> crate::Result<Output> {
    if !follow {
        let (events, next) = poll(after, wait, &kinds, context)?;
        let text = events
            .iter()
            .map(|event| line(event, next.instance))
            .collect::<Vec<_>>()
            .join("\n");
        let output = Output::data(
            if events.is_empty() {
                format!("No events; continue with --after {next}")
            } else {
                format!("{text}\nContinue with --after {next}")
            },
            &serde_json::json!({ "events": events, "next": next.to_string() }),
        )?;
        return Ok(if events.is_empty() {
            output.nothing()
        } else {
            output
        });
    }
    // One line per event, JSON lines with --json, until interrupted or the
    // app goes away.
    let mut after = after;
    let mut stdout = std::io::stdout().lock();
    loop {
        let (events, next) = poll(after, FOLLOW_WAIT, &kinds, context)?;
        for event in &events {
            // Each line carries its cursor, so a follower can resume after
            // any event, even across an app restart.
            let text = if context.json {
                let mut value = serde_json::to_value(event)?;
                if let Some(object) = value.as_object_mut() {
                    let cursor = Cursor {
                        instance: next.instance,
                        seq: event.seq,
                    };
                    object.insert("cursor".into(), cursor.to_string().into());
                }
                value.to_string()
            } else {
                line(event, next.instance)
            };
            writeln!(stdout, "{text}")?;
        }
        stdout.flush()?;
        after = Some(next);
    }
}

/// How often `wait window` looks for an app that is not running yet.
const START_POLL: std::time::Duration = std::time::Duration::from_millis(250);

pub(super) fn wait(what: WaitFor, timeout: u64, context: &Context) -> crate::Result<Output> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout);
    loop {
        let left = deadline
            .saturating_duration_since(std::time::Instant::now())
            .as_secs()
            .max(1);
        match wait_once(what, left, context) {
            // `wait window` also waits for the app itself to start.
            Err(crate::Error::Control(crate::control::Error::Unavailable { .. }))
                if what == WaitFor::Window && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(START_POLL);
            }
            result => return result,
        }
    }
}

fn wait_once(what: WaitFor, timeout: u64, context: &Context) -> crate::Result<Output> {
    let condition = match what {
        WaitFor::Feedback => return super::browser::feedback(timeout, context),
        WaitFor::Window => Condition::Window,
        WaitFor::Online => Condition::Online,
        WaitFor::TabClosed(id) => Condition::TabClosed { id },
    };
    match context.call(
        Request::Events,
        EventsOp::Wait {
            condition,
            wait_seconds: timeout,
        },
    )? {
        Response::Events {
            reply: Reply::Waited { met: true },
        } => Ok(Output::data("Done", &serde_json::json!({ "met": true }))?),
        Response::Events {
            reply: Reply::Waited { met: false },
        } => Ok(Output::data(
            format!("Still waiting after {timeout} seconds"),
            &serde_json::json!({ "met": false }),
        )?
        .nothing()),
        response => Err(unexpected(response)),
    }
}
