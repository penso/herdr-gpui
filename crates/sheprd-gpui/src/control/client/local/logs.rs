//! `logs`, answered by this process from the app's log files.
use super::Output;
use crate::diagnostics::{Record, Tail, log_file, previous_log_file};
use std::{path::Path, sync::Arc};

pub(crate) fn logs(path: bool, lines: usize) -> crate::Result<Output> {
    let current = log_file().ok_or(crate::Error::MissingStateRoot)?;
    let previous = previous_log_file(&current);
    if path {
        return Output::data(
            current.display().to_string(),
            &serde_json::json!({
                "path": current.display().to_string(),
                "previous": previous.display().to_string(),
            }),
        );
    }
    let records = tail(&previous, &current, lines)?;
    if records.is_empty() {
        return Ok(Output::message(format!("No log yet at {}", current.display())).nothing());
    }
    let records: Vec<&Record> = records.iter().map(Arc::as_ref).collect();
    Output::data(lines_of(&records), &records)
}

/// The newest `lines` records, oldest first, reaching into the rotated file
/// only when the current one was read whole and has fewer: a reader keeps
/// the end of a long file, and older records must never stand in for the
/// newer ones it skipped.
pub(super) fn tail(
    previous: &Path,
    current: &Path,
    lines: usize,
) -> crate::Result<Vec<Arc<Record>>> {
    let mut records = read(current)?;
    let whole = std::fs::metadata(current).map_or(true, |metadata| {
        metadata.len() <= crate::diagnostics::TAIL_BYTES
    });
    if records.len() < lines && records.len() < crate::diagnostics::TAIL_RECORDS && whole {
        let mut older = read(previous)?;
        older.append(&mut records);
        records = older;
    }
    let skip = records.len().saturating_sub(lines);
    records.drain(..skip);
    Ok(records)
}

fn read(path: &Path) -> crate::Result<Vec<Arc<Record>>> {
    let mut tail = Tail::new(path);
    tail.read().map_err(|source| crate::Error::LogRead {
        path: path.to_owned(),
        source,
    })?;
    Ok(tail.records())
}

pub(super) fn lines_of(records: &[&Record]) -> String {
    records
        .iter()
        .map(|record| record.line())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
