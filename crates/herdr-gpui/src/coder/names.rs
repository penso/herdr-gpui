//! Coder workspace names: 1-32 characters of `[a-z0-9]` in hyphen-separated runs.
//! Coder also accepts uppercase, but lowercase keeps names stable as SSH hosts.

pub(crate) const LIMIT: usize = 32;

pub(crate) fn valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= LIMIT
        && name.split('-').all(|run| {
            !run.is_empty() && run.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9'))
        })
}

/// A suggested name for a new workspace: the prefix, then the label slugged.
pub(crate) fn suggest(prefix: &str, label: &str) -> String {
    let mut name = prefix.to_owned();
    let mut pending = true;
    for c in label.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            if pending {
                name.push('-');
                pending = false;
            }
            name.push(c);
        } else {
            pending = true;
        }
        if name.len() >= LIMIT {
            break;
        }
    }
    name.truncate(LIMIT);
    name.trim_end_matches('-').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_coder_rules() {
        for name in ["a", "herdr-1", "a1-b2-c3", &"a".repeat(LIMIT)] {
            assert!(valid(name), "{name}");
        }
        for name in [
            "",
            "-a",
            "a-",
            "a--b",
            "A",
            "a_b",
            "a.b",
            &"a".repeat(LIMIT + 1),
        ] {
            assert!(!valid(name), "{name}");
        }
    }

    #[test]
    fn suggestions_are_always_valid() {
        assert_eq!(suggest("herdr", "My Dev Box!"), "herdr-my-dev-box");
        assert_eq!(suggest("herdr", "  "), "herdr");
        assert_eq!(suggest("herdr", "Café au lait"), "herdr-caf-au-lait");
        let long = suggest("herdr", &"word ".repeat(20));
        assert!(long.len() <= LIMIT);
        for label in [
            "x-".repeat(40),
            "é".repeat(40),
            "a b c".into(),
            "---".into(),
        ] {
            assert!(valid(&suggest("herdr", &label)), "{label}");
        }
    }
}
