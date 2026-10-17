//! How host answers read in a terminal. Pure, so the layout is tested
//! without an app; `finish` makes the app's text safe to print.
use crate::control::protocol::{
    app::HostInfo,
    hosts::{CheckInfo, CommentInfo, CopyState, LoadInfo, Ports, PrInfo, RemoteCopy, Reply},
};
use std::fmt::Write as _;

fn copy_state(state: CopyState) -> &'static str {
    match state {
        CopyState::Synced => "synced",
        CopyState::Edited => "edited",
        CopyState::Asking => "asking",
        CopyState::Uploading => "uploading",
        CopyState::Conflict => "conflict",
        CopyState::Declined => "declined",
        CopyState::Refetching => "refetching",
    }
}

/// One line per copy: its state, the host's file, and the copy here.
pub(super) fn remote_files(copies: &[RemoteCopy], omitted: usize) -> String {
    if copies.is_empty() && omitted == 0 {
        return "No remote file copies.".to_owned();
    }
    let mut text = String::new();
    for copy in copies {
        let auto = if copy.auto_upload {
            " (auto-upload)"
        } else {
            ""
        };
        let _ = writeln!(
            text,
            "{}{auto}  {}:{}  -> {}",
            copy_state(copy.state),
            copy.host,
            copy.path,
            copy.local
        );
    }
    if omitted > 0 {
        let _ = writeln!(text, "… and {omitted} more copies that did not fit");
    }
    text.trim_end().to_owned()
}

pub(super) fn pr(pr: &PrInfo) -> String {
    let mut text = format!("#{} {}\n", pr.number, pr.title);
    let _ = writeln!(
        text,
        "{} · {} -> {} · +{} -{} in {} {}",
        pr.state,
        pr.head,
        pr.base,
        pr.additions,
        pr.deletions,
        pr.changed_files,
        if pr.changed_files == 1 {
            "file"
        } else {
            "files"
        }
    );
    let details: Vec<&str> = [pr.review.as_deref(), pr.merge_status.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if !details.is_empty() {
        let _ = writeln!(text, "{}", details.join(" · "));
    }
    let _ = writeln!(text, "Checks: {}", summary(&pr.checks));
    text.push_str(&pr.url);
    text
}

/// `2 passed, 1 failed`, in the order outcomes first appear.
fn summary(checks: &[CheckInfo]) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for check in checks {
        match counts
            .iter_mut()
            .find(|(outcome, _)| *outcome == check.outcome)
        {
            Some((_, count)) => *count += 1,
            None => counts.push((&check.outcome, 1)),
        }
    }
    if counts.is_empty() {
        return "none reported".into();
    }
    counts
        .iter()
        .map(|(outcome, count)| format!("{count} {outcome}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn checks(checks: &[CheckInfo]) -> String {
    if checks.is_empty() {
        return "No checks reported".into();
    }
    checks
        .iter()
        .map(|check| format!("{:<8} {}", check.outcome, check.name))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn comments(comments: &[CommentInfo]) -> String {
    if comments.is_empty() {
        return "No comments yet".into();
    }
    comments
        .iter()
        .map(|comment| {
            let mut entry = format!(
                "{} {} ({})",
                comment.author, comment.kind, comment.created_at
            );
            if !comment.body.is_empty() {
                entry.push_str("\n    ");
                entry.push_str(&comment.body);
            }
            entry
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn hosts(hosts: &[HostInfo]) -> String {
    if hosts.is_empty() {
        return "The window has no hosts".into();
    }
    hosts
        .iter()
        .map(|host| {
            format!(
                "{} {} ({}): {}",
                if host.selected { ">" } else { " " },
                host.label,
                host.id,
                host.status
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn ports(ports: &Ports) -> String {
    let Ports {
        host,
        workspace,
        listening,
        forwards,
        note,
    } = ports;
    let mut text = String::new();
    if listening.is_empty() {
        let _ = writeln!(text, "Workspace {workspace} on {host} listens on no port");
    } else {
        let _ = writeln!(text, "Workspace {workspace} on {host} listens on:");
    }
    for port in listening {
        let _ = write!(text, "  {:<24}", port.bind);
        if let Some(process) = &port.process {
            let _ = write!(text, " {process}");
        }
        if let Some(url) = &port.local_url {
            let _ = write!(text, "  {url}");
        }
        text.push('\n');
    }
    if !forwards.is_empty() {
        text.push_str("Forwarded:\n");
    }
    for forward in forwards {
        let _ = write!(
            text,
            "  {}:{} {}",
            forward.host, forward.port, forward.state
        );
        if let Some(url) = &forward.local_url {
            let _ = write!(text, "  {url}");
        }
        if let Some(reason) = &forward.reason {
            let _ = write!(text, " ({reason})");
        }
        text.push('\n');
    }
    if let Some(note) = note {
        text.push_str(note);
    }
    text.trim_end().to_owned()
}

/// `2h 53m`, `5m`, `3d 4h`: the two largest units.
pub(super) fn duration(seconds: u64) -> String {
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60);
    match (days, hours) {
        (0, 0) => format!("{minutes}m"),
        (0, _) => format!("{hours}h {minutes}m"),
        _ => format!("{days}d {hours}h"),
    }
}

pub(super) fn usage(reply: &Reply) -> String {
    let Reply::Usage { host, usage, note } = reply else {
        return String::new();
    };
    let mut text = String::new();
    if usage.is_empty() {
        let _ = writeln!(text, "No usage to show on {host}");
    }
    for provider in usage {
        let _ = write!(text, "{}", provider.provider);
        for part in [&provider.plan, &provider.account].into_iter().flatten() {
            let _ = write!(text, " · {part}");
        }
        text.push('\n');
        for window in &provider.windows {
            let _ = write!(
                text,
                "  {:<12} {:>3.0}% used",
                window.label, window.used_percent
            );
            if let Some(left) = window.resets_in_seconds {
                let _ = write!(text, ", resets in {}", duration(left));
            }
            text.push('\n');
        }
        if let Some(error) = &provider.error {
            let _ = writeln!(text, "  {error}");
        }
    }
    if let Some(note) = note {
        text.push_str(note);
    }
    text.trim_end().to_owned()
}

/// `1.5 GiB`, in binary units as the status bar shows memory.
pub(in crate::control::client) fn bytes(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB"] {
        if value < 1024. {
            break;
        }
        value /= 1024.;
        unit = next;
    }
    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {unit}")
    }
}

fn load(load: &LoadInfo) -> String {
    let mut lines = vec![load.host.clone()];
    if let Some(cpu) = load.cpu_percent {
        let cores = load
            .cores
            .map(|cores| format!(" of {cores} cores"))
            .unwrap_or_default();
        lines.push(format!("  CPU     {cpu:.0}%{cores}"));
    }
    if let (Some(used), Some(total)) = (load.memory_used, load.memory_total) {
        lines.push(format!("  Memory  {} of {}", bytes(used), bytes(total)));
    }
    if let (Some(available), Some(total)) = (load.disk_available, load.disk_total) {
        lines.push(format!(
            "  Disk    {} free of {}",
            bytes(available),
            bytes(total)
        ));
    }
    if let Some([one, five, fifteen]) = load.load {
        lines.push(format!("  Load    {one:.2} {five:.2} {fifteen:.2}"));
    }
    if let Some(uptime) = load.uptime_seconds {
        lines.push(format!("  Up      {}", duration(uptime)));
    }
    lines.join("\n")
}

pub(super) fn system(reply: &Reply) -> String {
    let Reply::System { load: sample, note } = reply else {
        return String::new();
    };
    let mut text = sample.as_deref().map(load).unwrap_or_default();
    if let Some(note) = note {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(note);
    }
    text
}
