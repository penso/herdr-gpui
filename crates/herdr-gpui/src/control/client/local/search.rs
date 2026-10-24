//! `search`, answered by this process: Go to Symbol and Go to File over the
//! current directory's checkout, indexed and ranked as the app's picker does.
use super::Output;
use crate::{
    code_index::{self, Index},
    code_search::{Mode, Query, rank},
    control::ErrorCode,
};
use serde::Serialize;
use std::{path::Path, sync::atomic::AtomicBool};

/// One match: a file, or a definition in one. Paths are relative to the
/// checkout's root.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct Found {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<&'static str>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct Results {
    pub root: String,
    pub matches: Vec<Found>,
    /// Whether the index hit a size limit and left files out.
    pub truncated: bool,
}

/// The best `limit` matches of `query` in `index`, best first.
pub(super) fn find(index: &Index, files: bool, query: &str, limit: usize) -> Vec<Found> {
    let mode = if files { Mode::Files } else { Mode::Symbols };
    let query = Query::parse(query, mode);
    rank(index, mode, &query)
        .into_iter()
        .filter_map(|hit| match mode {
            Mode::Files => Some(Found {
                path: index.files().get(hit.item)?.clone(),
                line: query.line,
                name: None,
                kind: None,
            }),
            Mode::Symbols => {
                let symbol = index.symbols().get(hit.item)?;
                Some(Found {
                    path: index.files().get(symbol.file)?.clone(),
                    line: Some(symbol.line),
                    name: Some(symbol.name.clone()),
                    kind: Some(symbol.kind.label()),
                })
            }
        })
        .take(limit)
        .collect()
}

pub(super) fn text(matches: &[Found]) -> String {
    matches
        .iter()
        .map(|found| {
            let mut line = found.path.clone();
            if let Some(number) = found.line {
                line.push_str(&format!(":{number}"));
            }
            if let Some(name) = &found.name {
                line.push_str(&format!("  {name}"));
            }
            if let Some(kind) = found.kind {
                line.push_str(&format!(" ({kind})"));
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Indexes the checkout holding `dir` and searches it. Blocking, which the
/// command line may be.
pub(super) fn search_in(
    dir: &Path,
    files: bool,
    query: &str,
    limit: usize,
) -> crate::Result<Output> {
    let cancelled = AtomicBool::new(false);
    let dir = dir
        .to_str()
        .ok_or_else(|| crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            message: "The current directory's path is not UTF-8".into(),
        })?;
    let root = code_index::checkout_root(dir, &cancelled)?;
    let index = Index::build(&root, &cancelled)?;
    let results = Results {
        root: root.display().to_string(),
        matches: find(&index, files, query, limit),
        truncated: index.truncated(),
    };
    if results.matches.is_empty() {
        let what = if files { "file" } else { "symbol" };
        return Ok(Output::data(
            format!("No {what} in {} matches {query}", results.root),
            &results,
        )?
        .nothing());
    }
    Output::data(text(&results.matches), &results)
}

pub(crate) fn search(files: bool, query: &str, limit: usize) -> crate::Result<Output> {
    search_in(&std::env::current_dir()?, files, query, limit)
}

#[cfg(test)]
mod tests;
