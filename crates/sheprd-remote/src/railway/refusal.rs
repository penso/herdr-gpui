//! What Railway prints when it refuses a headless connection (exit 13): a
//! one-line JSON object, observed live; Railway documents no schema. Only the
//! `status` and `description` fields are read, and the links such output can
//! carry act on the VM or its owner, so they are never stored, logged, shown,
//! or opened.

/// Why Railway refused a headless connection (exit 13).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    /// The key has no VM yet and Railway says only a connection with a
    /// terminal may create one. The one refusal a terminal session can fix.
    NeedsTerminal,
    /// Anything else, such as the daily limit on new VMs: a terminal session
    /// would not change it, so none is started.
    Other,
}

/// What a refused connection's output says. Only a JSON object whose
/// `status` is `refused` and whose `description` says a headless connection
/// is not allowed counts as [`Refusal::NeedsTerminal`]; unknown or changed
/// wording is `Other`, which fails safe by never starting a terminal session.
pub(super) fn refusal(output: &str) -> Refusal {
    let needs_terminal = output
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('{') && line.len() <= 8192)
        .any(|line| {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                return false;
            };
            value.get("status").and_then(|status| status.as_str()) == Some("refused")
                && value
                    .get("description")
                    .and_then(|text| text.as_str())
                    .is_some_and(|text| text.to_ascii_lowercase().contains("headless"))
        });
    if needs_terminal {
        Refusal::NeedsTerminal
    } else {
        Refusal::Other
    }
}

/// `text` with every `http(s)://…` word replaced, for remote output shown to
/// the user: a failure's output can carry such a link.
pub(super) fn hide_links(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            line.split(' ')
                .map(|word| {
                    // JSON puts the link after a quote or colon. The earliest of
                    // either scheme, so nothing before a later link survives.
                    match ["http://", "https://"]
                        .iter()
                        .filter_map(|scheme| word.find(scheme))
                        .min()
                    {
                        Some(start) => format!("{}[link hidden]", &word[..start]),
                        None => word.to_owned(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
