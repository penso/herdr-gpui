//! Conservative redaction for a diagnostics bundle: the home directory reads
//! `~`, and anything that names or looks like a credential is replaced. It
//! would rather hide a harmless value than leak a secret.

const REDACTED: &str = "[redacted]";

/// Names whose values are secrets, matched within a key, case-insensitively.
const SECRET_NAMES: [&str; 11] = [
    "token",
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "auth",
    "cookie",
    "private_key",
    "credential",
    "session_key",
];

/// Prefixes of well-known token formats.
const TOKEN_PREFIXES: [&str; 11] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "sk-",
    "xoxb-",
    "xoxp-",
    "AKIA",
];

/// Unbroken runs at least this long that mix letters and digits are taken
/// for keys or tokens; names such as branches break theirs with `-`.
const OPAQUE_LENGTH: usize = 24;

pub(super) struct Redactor {
    home: Option<String>,
}

pub(super) fn secret_name(name: &str) -> bool {
    let name = name.trim().trim_matches(['"', '\'']).to_ascii_lowercase();
    !name.is_empty() && SECRET_NAMES.iter().any(|secret| name.contains(secret))
}

fn opaque_run(run: &str) -> bool {
    run.len() >= OPAQUE_LENGTH
        && !run.starts_with(['/', '~'])
        && run
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/'))
        && run.chars().any(|c| c.is_ascii_digit())
        && run.chars().any(|c| c.is_ascii_alphabetic())
}

/// Whether `word`, or the value it assigns, looks like a key or a token.
fn opaque(word: &str) -> bool {
    if word.contains("://") {
        return false;
    }
    word.split(['=', ':']).any(|part| {
        let part = part
            .trim_matches(|c: char| matches!(c, '"' | '\'' | ',' | ';' | ')' | '(' | '[' | ']'));
        TOKEN_PREFIXES
            .iter()
            .any(|prefix| part.starts_with(prefix) && part.len() > prefix.len() + 8)
            || (!part.starts_with(['/', '~', '.']) && part.split(['-', '_', '.']).any(opaque_run))
    })
}

/// `scheme://user:password@host` without its user information.
fn without_userinfo(word: &str) -> Option<String> {
    let start = word.find("://")? + 3;
    let rest = &word[start..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let at = rest[..end].rfind('@')?;
    Some(format!("{}{REDACTED}{}", &word[..start], &rest[at..]))
}

impl Redactor {
    pub(super) fn new(home: Option<&std::path::Path>) -> Self {
        Self {
            home: home
                .and_then(std::path::Path::to_str)
                .map(|home| home.trim_end_matches('/').to_owned())
                .filter(|home| home.len() > 1),
        }
    }

    pub(super) fn redact(&self, text: &str) -> String {
        text.lines()
            .map(|line| self.line(line))
            .collect::<Vec<_>>()
            .join("\n")
            + if text.ends_with('\n') { "\n" } else { "" }
    }

    fn line(&self, line: &str) -> String {
        let line = match &self.home {
            Some(home) => line.replace(home.as_str(), "~"),
            None => line.to_owned(),
        };
        // A TOML assignment whose key names a secret keeps only its key.
        if let Some((key, _)) = line.split_once('=')
            && !key.trim().contains(' ')
            && secret_name(key.trim().rsplit('.').next().unwrap_or_default())
        {
            return format!("{}= \"{REDACTED}\"", key);
        }
        let mut redacting = false;
        let mut words = Vec::new();
        let mut previous_secret = false;
        for word in line.split(' ') {
            if redacting {
                // The rest of a quoted secret that held spaces.
                redacting = !word.ends_with('"');
                words.push(REDACTED.to_owned());
                continue;
            }
            if previous_secret && !word.is_empty() {
                previous_secret = false;
                words.push(REDACTED.to_owned());
                continue;
            }
            if let Some(index) = word.find(['=', ':'])
                && secret_name(&word[..index])
                && !word[index + 1..].starts_with("//")
            {
                let value = &word[index + 1..];
                if value.is_empty() {
                    // `password: hunter2`: the value is the next word.
                    previous_secret = true;
                    words.push(word.to_owned());
                } else {
                    redacting =
                        value.starts_with('"') && (value.len() < 2 || !value.ends_with('"'));
                    words.push(format!("{}{REDACTED}", &word[..=index]));
                }
                continue;
            }
            if matches!(
                word.to_ascii_lowercase().as_str(),
                "bearer" | "basic" | "token"
            ) {
                previous_secret = true;
                words.push(word.to_owned());
                continue;
            }
            if opaque(word) {
                words.push(REDACTED.to_owned());
            } else if let Some(cleaned) = without_userinfo(word) {
                words.push(cleaned);
            } else {
                words.push(word.to_owned());
            }
        }
        words.join(" ")
    }
}
