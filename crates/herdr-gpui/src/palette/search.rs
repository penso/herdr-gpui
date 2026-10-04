//! Prepared search fields keep ranking independent of rendering and input.

pub(super) struct Fields {
    name: String,
    context: String,
}

impl Fields {
    pub(super) fn new(name: &str, context: &str) -> Self {
        Self {
            name: name.to_lowercase(),
            context: context.to_lowercase(),
        }
    }

    pub(super) fn score(&self, terms: &[&str]) -> Option<(usize, usize)> {
        terms.iter().try_fold((0, 0), |(quality, primary), term| {
            let name = match_quality(&self.name, term);
            let context = match_quality(&self.context, term);
            let best = name.max(context);
            (best > 0).then_some((quality + best, primary + usize::from(name == best)))
        })
    }
}

fn match_quality(text: &str, term: &str) -> usize {
    if text == term {
        return 4;
    }
    if text
        .split(|ch: char| !ch.is_alphanumeric())
        .any(|word| word.starts_with(term))
    {
        return 3;
    }
    if text.contains(term) {
        return 2;
    }
    let mut remaining = term.chars();
    let mut wanted = remaining.next();
    for ch in text.chars() {
        if wanted == Some(ch) {
            wanted = remaining.next();
            if wanted.is_none() {
                return 1;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_exact_prefix_substring_and_fuzzy_matches() {
        let score = |name| Fields::new(name, "").score(&["build"]);
        assert!(score("build") > score("Build project"));
        assert!(score("Build project") > score("rebuilding"));
        assert!(score("rebuilding") > score("busy idle logs daemon"));
        assert_eq!(score("missing"), None);
        assert!(
            Fields::new("build", "").score(&["build"])
                > Fields::new("other", "build").score(&["build"])
        );
    }

    #[test]
    fn matches_unicode_and_all_terms_across_name_and_context() {
        let fields = Fields::new("CAFÉ ΑΒ", "Box workspace /repo");
        assert!(fields.score(&["café", "αβ", "box"]).is_some());
        assert!(fields.score(&["box", "café"]).is_some());
        assert!(fields.score(&["missing", "café"]).is_none());
        assert_eq!(fields.score(&[]), Some((0, 0)));
    }

    #[test]
    fn fuzzy_matching_does_not_join_name_with_context() {
        assert!(Fields::new("ab", "cd").score(&["abcd"]).is_none());
    }
}
