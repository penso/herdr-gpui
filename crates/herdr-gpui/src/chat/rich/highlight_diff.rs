//! Diff syntax uses the existing grammar worker, with independent before/after
//! streams. Patch signs stay in the displayed source and shift token offsets.
use super::diff::{self, Kind};
use crate::review::highlight::{Span, colour_file};

pub(super) fn highlight(source: &str) -> Vec<Vec<Span>> {
    if source.len() > super::MAX_CODE_BYTES {
        return Vec::new();
    }
    let rows = diff::rows(source, super::MAX_CODE_LINES);
    let text: Vec<_> = source.lines().take(rows.len()).collect();
    let mut result = vec![Vec::new(); rows.len()];
    let mut start = 0;
    while start < rows.len() {
        if rows[start].prefix != 1 || rows[start].path.is_none() {
            start += 1;
            continue;
        }
        let end = (start + 1..rows.len())
            .find(|&index| rows[index].prefix != 1 && !text[index].starts_with("\\ No newline"))
            .unwrap_or(rows.len());
        // A metadata/hunk boundary resets the grammar: omitted context must not
        // carry an open string or comment into another supplied fragment.
        for old in [true, false] {
            let preferred = if old { Kind::Removed } else { Kind::Added };
            let path = rows[start..end]
                .iter()
                .find(|row| row.kind == preferred && row.path.is_some())
                .and_then(|row| row.path)
                .or(rows[start].path)
                .unwrap_or("");
            let indices: Vec<_> = (start..end)
                .filter(|&index| match rows[index].kind {
                    Kind::Removed => old,
                    Kind::Added => !old,
                    Kind::Context => true,
                    Kind::Meta => false,
                })
                .collect();
            let mut side = String::new();
            for &index in &indices {
                side.push_str(text[index].get(1..).unwrap_or(""));
                side.push('\n');
            }
            for (index, spans) in indices
                .into_iter()
                .zip(colour_file(path, &side, end - start))
            {
                if !old || rows[index].kind == Kind::Removed {
                    result[index] = spans
                        .into_iter()
                        .map(|span| Span {
                            start: span.start + 1,
                            end: span.end + 1,
                            token: span.token,
                        })
                        .collect();
                }
            }
        }
        start = end;
    }
    result
}

#[cfg(test)]
mod tests;
