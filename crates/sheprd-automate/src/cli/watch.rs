//! Text for people: `status` and `usage` as plain text, `watch` as a live
//! dashboard that redraws every two seconds, and `log --daemon --follow`.
//! All read-only; pausing and resuming stay separate commands.

use crate::{Error, Result, paths::Paths, report, runner::now, settings::Settings, store::Store};
use sheprd_automation::report::{RunState, ScriptState, Status, UsageReport};
use std::{
    fmt::Write as _,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom, Write},
    path::Path,
    thread,
    time::Duration,
};

const REDRAW: Duration = Duration::from_secs(2);

pub fn run(paths: &Paths, out: &mut impl Write) -> Result<()> {
    loop {
        let store = Store::open(&paths.database())?;
        let settings = Settings::read(&paths.settings())?;
        let status = report::status(paths, &store, &settings, now())?;
        // Clear the screen and draw from the top.
        write!(out, "\x1b[2J\x1b[H{}\n(Ctrl-C to leave)\n", render(&status))
            .map_err(Error::io("stdout"))?;
        out.flush().map_err(Error::io("stdout"))?;
        thread::sleep(REDRAW);
    }
}

/// Prints the daemon log's last `lines`, then each new line as it comes.
pub fn follow(log: &Path, lines: usize, out: &mut impl Write) -> Result<()> {
    let (text, _) = crate::process::tail_lines(log, 256 * 1024, lines).unwrap_or_default();
    if !text.is_empty() {
        writeln!(out, "{text}").map_err(Error::io("stdout"))?;
    }
    let mut offset = std::fs::metadata(log)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    loop {
        thread::sleep(Duration::from_secs(1));
        let Ok(mut file) = File::open(log) else {
            continue;
        };
        let size = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        if size < offset {
            // Rotated: read the new file from its start.
            offset = 0;
        }
        if size == offset {
            continue;
        }
        file.seek(SeekFrom::Start(offset)).map_err(Error::io(log))?;
        for line in BufReader::new(&mut file)
            .lines()
            .map_while(std::result::Result::ok)
        {
            writeln!(out, "{line}").map_err(Error::io("stdout"))?;
        }
        out.flush().map_err(Error::io("stdout"))?;
        offset = size;
    }
}

pub fn render(status: &Status) -> String {
    let mut text = String::new();
    let daemon = &status.daemon;
    let state = if daemon.running {
        format!("running, pid {}", daemon.pid.unwrap_or_default())
    } else {
        "not running (sheprd-automate start)".to_owned()
    };
    let _ = writeln!(
        text,
        "sheprd-automate {} · daemon {state}{}{}",
        status.version,
        if daemon.paused_all {
            " · scheduling paused"
        } else {
            ""
        },
        if daemon.service {
            " · service installed"
        } else {
            ""
        },
    );
    let _ = writeln!(
        text,
        "today: {} of {} tokens · at most {} automated agents",
        status.today.total(),
        status.caps.max_tokens_per_day,
        status.caps.max_agents
    );
    if status.scripts.is_empty() {
        let _ = writeln!(
            text,
            "\nno scripts: add a repository (`repo add PATH`) or a personal script"
        );
    }
    for script in &status.scripts {
        let state = match script.state {
            ScriptState::Active => "active",
            ScriptState::AwaitingApproval => "awaiting approval",
            ScriptState::Paused => "paused",
            ScriptState::Disabled => "disabled",
            ScriptState::Invalid => "invalid",
        };
        let reason = script
            .reason
            .as_deref()
            .map(|reason| format!(": {reason}"))
            .unwrap_or_default();
        let _ = writeln!(
            text,
            "\n{} [{state}{reason}]\n  {}",
            script.name, script.path
        );
        for trigger in &script.triggers {
            let next = trigger.next.map(when).unwrap_or_default();
            let name = trigger
                .name
                .as_deref()
                .map(|name| format!("{name}: "))
                .unwrap_or_default();
            let zone = if trigger.zone.is_empty() {
                String::new()
            } else {
                format!(" ({})", trigger.zone)
            };
            let next = if next.is_empty() {
                String::new()
            } else {
                format!(" · next {next}")
            };
            let _ = writeln!(text, "  {name}{}{zone}{next}", trigger.schedule);
        }
        if let Some(run) = script.running {
            let _ = writeln!(text, "  running: run #{run}");
        } else if let Some(run) = &script.last_run {
            let _ = writeln!(
                text,
                "  last run: #{} {:?} at {}",
                run.id,
                run.state,
                when(run.started_at)
            );
        }
        if let Some(tokens) = script.tokens_today {
            let _ = writeln!(text, "  tokens today: {tokens}");
        }
    }
    let recent = status.runs.iter().take(10).collect::<Vec<_>>();
    if !recent.is_empty() {
        let _ = writeln!(text, "\nrecent runs");
        for run in recent {
            let mark = match run.state {
                RunState::Running => "…",
                RunState::Completed => "✓",
                RunState::Stopped | RunState::Halted | RunState::Failed => "✗",
                RunState::Interrupted => "!",
            };
            let detail = run
                .detail
                .as_deref()
                .map(|detail| format!(" · {detail}"))
                .unwrap_or_default();
            let _ = writeln!(
                text,
                "  {mark} #{} {} at {}{detail}",
                run.id,
                run.name,
                when(run.started_at)
            );
        }
    }
    text
}

pub fn render_usage(usage: &UsageReport) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "by {} since {}", usage.by, when(usage.since));
    for row in &usage.rows {
        let tokens = row
            .usage
            .tokens
            .map_or("unknown".to_owned(), |tokens| tokens.total().to_string());
        let cpu = row
            .usage
            .cpu_ms
            .map_or("unknown".to_owned(), |cpu| format!("{}s", cpu / 1_000));
        let memory = row
            .usage
            .peak_memory
            .map_or("unknown".to_owned(), |memory| {
                format!("{} MB", memory / 1_000_000)
            });
        let _ = writeln!(
            text,
            "  {}: {tokens} tokens · {cpu} CPU · {memory} peak · {} run(s)",
            row.label, row.runs
        );
    }
    let total = usage
        .total
        .tokens
        .map_or("unknown".to_owned(), |tokens| tokens.total().to_string());
    let _ = write!(text, "total: {total} tokens");
    text
}

/// A Unix time in the host's zone, to the minute.
fn when(seconds: i64) -> String {
    jiff::Timestamp::from_second(seconds)
        .map(|at| {
            at.to_zoned(jiff::tz::TimeZone::system())
                .strftime("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
