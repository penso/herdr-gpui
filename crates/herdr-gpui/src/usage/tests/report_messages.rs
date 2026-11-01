//! Every message `herdr-usage` names with `Text::message`, checked against
//! the English files with the arguments chained to it, since a crate below
//! the app cannot use `fl!`'s compile-time check.
use crate::i18n;
use std::path::Path;

/// `(file, id, argument names)` for each `Text::message("…")` in `source`,
/// with the names of the `.arg("…", …)` calls chained right after it.
fn named(file: &str, source: &str) -> Vec<(String, String, Vec<String>)> {
    const CALL: &str = "Text::message(";
    let mut found = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find(CALL) {
        rest = &rest[start + CALL.len()..];
        let Some(id) = literal(rest) else {
            continue;
        };
        let mut after = &rest[closing(rest)..];
        let mut args = Vec::new();
        while let Some(call) = after.trim_start().strip_prefix(".arg(") {
            let Some(name) = literal(call) else { break };
            args.push(name.to_owned());
            after = &call[closing(call)..];
        }
        found.push((file.to_owned(), id.to_owned(), args));
    }
    found
}

/// The string literal `text` starts with, after whitespace.
fn literal(text: &str) -> Option<&str> {
    let text = text.trim_start().strip_prefix('"')?;
    Some(&text[..text.find('"')?])
}

/// Where the call whose arguments `text` starts with ends, past its `)`.
fn closing(text: &str) -> usize {
    let mut depth = 1;
    let mut quoted = false;
    for (index, char) in text.char_indices() {
        match char {
            '"' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

fn sources(dir: &Path, found: &mut Vec<(String, String, Vec<String>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path).unwrap();
            found.extend(named(&path.display().to_string(), &source));
        }
    }
}

#[test]
fn every_message_herdr_usage_names_exists_with_its_arguments() {
    let mut found = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../herdr-usage/src"),
        &mut found,
    );
    for (file, id, mut args) in found {
        let Some(takes) = i18n::takes(&id) else {
            panic!("{file}: no English message {id}");
        };
        let mut takes = takes.to_vec();
        args.sort_unstable();
        takes.sort_unstable();
        assert_eq!(args, takes, "{file}: {id} takes these arguments");
    }
}

#[test]
fn the_scan_reads_chained_arguments() {
    let source = r#"
        Text::message("usage-report-a"),
        Text::message("usage-report-b")
            .arg("count", models.len().max(1))
            .arg("name", Text::service(name)),
        Text::message( "usage-report-c" ).to_owned()
    "#;
    let found = named("x.rs", source);
    let ids: Vec<_> = found
        .iter()
        .map(|(_, id, args)| (id.as_str(), args.join(",")))
        .collect();
    assert_eq!(
        ids,
        [
            ("usage-report-a", String::new()),
            ("usage-report-b", "count,name".into()),
            ("usage-report-c", String::new()),
        ]
    );
}
