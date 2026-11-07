//! `scripts list`: the worktree scripts of the current directory's checkout,
//! read in this process with the app's own parser. Reading never runs them.
use super::{super::Output, here, rejected};
use crate::worktree_scripts::{CONFIG_PATH, Config, ScriptKind, read_config};
use serde_json::{Map, Value};
use sheprd_client::ConnectTarget;
use std::{fmt::Write as _, path::Path, sync::atomic::AtomicBool};

/// What `scripts list` prints for the file read from `root`, if any: text
/// with each script's first line, and JSON with each script whole.
pub(super) fn listing(root: &Path, config: Option<&Config>) -> (String, Value) {
    let root_text = root.display().to_string();
    let Some(config) = config else {
        return (
            format!("No {CONFIG_PATH} in {root_text}"),
            serde_json::json!({ "root": root_text, "file": CONFIG_PATH, "present": false, "scripts": {} }),
        );
    };
    let mut text = String::new();
    let mut scripts = Map::new();
    for kind in ScriptKind::ALL {
        let script = config.scripts.get(kind);
        let first = script
            .and_then(|script| script.lines().find(|line| !line.trim().is_empty()))
            .map_or("(none)", str::trim);
        let more = script.is_some_and(|script| script.trim().lines().count() > 1);
        let _ = writeln!(
            text,
            "{:<8}{first}{}",
            kind.name(),
            if more { " ..." } else { "" }
        );
        scripts.insert(
            kind.name().to_owned(),
            script.map_or(Value::Null, |script| Value::String(script.to_owned())),
        );
    }
    (
        text.trim_end().to_owned(),
        serde_json::json!({
            "root": root_text,
            "file": CONFIG_PATH,
            "present": true,
            "digest": config.digest,
            "scripts": scripts,
        }),
    )
}

pub(super) fn list() -> crate::Result<Output> {
    let dir = std::env::current_dir().map_err(|error| rejected(crate::Error::LocalGit(error)))?;
    let root = here::root(&dir).map_err(rejected)?;
    let checkout = root
        .to_str()
        .ok_or_else(|| rejected(crate::Error::NotInCheckout))?;
    let config =
        read_config(&ConnectTarget::Local, checkout, &AtomicBool::new(false)).map_err(rejected)?;
    let (text, json) = listing(&root, config.as_ref());
    Output::data(text, &json)
}
