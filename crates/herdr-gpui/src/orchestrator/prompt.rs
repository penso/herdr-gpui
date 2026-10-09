//! The prompt a dispatched agent starts with, composed as agent-launcher
//! composes it: a prompt profile from agent-launcher's
//! `~/.config/agent-launcher/agents/<name>/prompt.md` replaces the built-in
//! issue prompt, and extra instructions are appended verbatim. Both
//! applications therefore send the same text for the same choice.
//!
//! Profiles use `{{ issue_title }}`-style variables. Only those are filled
//! in here; other template syntax is left as written.

use super::{Item, Provider};
use std::path::Path;

/// The most profiles listed, and the largest one read.
const MAX_PROFILES: usize = 32;
const MAX_PROFILE: u64 = 64 * 1024;
/// agent-launcher's cap on extra instructions.
pub(crate) const MAX_EXTRA: usize = 16 * 1024;
/// The most of an item's description a prompt carries.
const MAX_DESCRIPTION: usize = 32 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    pub(crate) name: String,
    pub(crate) template: String,
}

/// The profiles under `home`, by name. Unreadable or oversized ones are
/// skipped: a profile is a convenience, never required.
pub(crate) fn load_profiles(home: &Path) -> Vec<Profile> {
    let root = home.join(".config").join("agent-launcher").join("agents");
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };
    let mut profiles: Vec<Profile> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().into_string().ok()?;
            let path = entry.path().join("prompt.md");
            let metadata = std::fs::metadata(&path).ok()?;
            if !metadata.is_file() || metadata.len() > MAX_PROFILE {
                return None;
            }
            let template = std::fs::read_to_string(path).ok()?;
            (!template.trim().is_empty()).then_some(Profile { name, template })
        })
        .collect();
    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    profiles.truncate(MAX_PROFILES);
    profiles
}

/// agent-launcher's built-in prompt for an issue.
pub(crate) fn built_in(item: &Item) -> String {
    let description = bounded(item.description.as_deref().unwrap_or(""), MAX_DESCRIPTION);
    format!(
        "Implement this issue.\n\nProvider: {}\nRepository: {}\nIdentifier: {}\nTitle: {}\nDescription: {}\nURL: {}",
        provider(item.key.source.provider),
        item.key.source.repository,
        item.identifier,
        item.title,
        description,
        item.url.as_deref().unwrap_or(""),
    )
}

/// The full prompt: `profile` rendered for `item` or the built-in prompt,
/// then `extra` on its own paragraph when given.
pub(crate) fn compose(item: &Item, profile: Option<&Profile>, extra: &str) -> String {
    let mut prompt = match profile {
        Some(profile) => render(&profile.template, item).trim().to_owned(),
        None => built_in(item),
    };
    let extra = bounded(extra, MAX_EXTRA);
    if !extra.trim().is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(extra);
    }
    prompt
}

/// Fills the variables agent-launcher defines into `template`.
pub(crate) fn render(template: &str, item: &Item) -> String {
    let description = bounded(item.description.as_deref().unwrap_or(""), MAX_DESCRIPTION);
    let value = |name: &str| -> Option<String> {
        Some(match name {
            "issue_text" => {
                if description.is_empty() {
                    "(no issue description provided)".to_owned()
                } else {
                    description.to_owned()
                }
            }
            "issue_title" => item.title.clone(),
            "issue_link" => item
                .url
                .clone()
                .unwrap_or_else(|| "(no issue link available)".to_owned()),
            "issue_identifier" => item.identifier.clone(),
            "issue_repository" => item.key.source.repository.clone(),
            "issue_provider" => provider(item.key.source.provider).to_owned(),
            _ => return None,
        })
    };
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        match value(after[..end].trim()) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

fn provider(provider: Provider) -> &'static str {
    provider.as_str()
}

/// `text` cut to at most `limit` bytes on a character boundary.
fn bounded(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// herdr-gpui's branch for a new checkout of `item`: the number or bead id,
/// then a slug of the title, as GitHub names an issue's branch.
pub(crate) fn branch(item: &Item) -> String {
    let id = match item.key.source.provider {
        Provider::Github | Provider::Gitlab => item.identifier.trim_start_matches('#').to_owned(),
        Provider::Beads => item.key.native_id.clone(),
    };
    let id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let mut slug = String::new();
    for c in item.title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 50 {
            break;
        }
    }
    let slug = slug.trim_matches('-');
    let id = id.trim_matches('-');
    match (id.is_empty(), slug.is_empty()) {
        (false, false) => format!("{id}-{slug}"),
        (false, true) => id.to_owned(),
        (true, false) => slug.to_owned(),
        (true, true) => "orchestrator".to_owned(),
    }
}

/// Whether `branch` is a name `git` accepts for a new branch, conservatively.
pub(crate) fn valid_branch(branch: &str) -> bool {
    !branch.is_empty()
        && branch.len() <= 200
        && !branch.starts_with(['-', '/', '.'])
        && !branch.ends_with(['/', '.'])
        && !branch.ends_with(".lock")
        && !branch.contains("..")
        && !branch.contains("//")
        && !branch.contains("@{")
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}
