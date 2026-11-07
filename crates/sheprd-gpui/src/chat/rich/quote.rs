//! Block quote markers at the start of a line, as CommonMark reads them: a
//! container's later lines repeat its markers, which belong to Markdown and
//! never to the code or formula inside.

/// `line` after one marker: leading spaces, the `>`, and one optional space.
fn strip_marker(line: &str) -> Option<&str> {
    let after = line.trim_start_matches(' ').strip_prefix('>')?;
    Some(after.strip_prefix(' ').unwrap_or(after))
}

/// `line` after `depth` markers, or `None` when it has fewer.
pub(super) fn strip(line: &str, depth: usize) -> Option<&str> {
    (0..depth).try_fold(line, |line, _| strip_marker(line))
}

/// How many markers open `line`.
pub(super) fn depth(line: &str) -> usize {
    std::iter::successors(Some(line), |line| strip_marker(line)).count() - 1
}

#[cfg(test)]
mod tests;
