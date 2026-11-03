use super::*;
use std::{fs, path::Path};

mod activity;
mod config;
mod example;
mod mods;
mod worker;

/// Writes a mod folder named `name` with `main` as its `main.luau`.
fn write_mod(root: &Path, name: &str, main: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("mod.toml"),
        format!("name = \"{name}\"\napi = 1\nplaces = [\"status_bar\"]\n"),
    )
    .unwrap();
    fs::write(dir.join("main.luau"), main).unwrap();
    dir
}

/// A mod showing `text`, or the number of times it was clicked.
fn counter(text: &str) -> String {
    format!(
        "local ui = require(\"@herdr/ui\")\n\
         return herdr.mod {{\n\
           watch = {{ \"agents\" }},\n\
           init = function() return {{ n = 0 }} end,\n\
           update = function(s, msg)\n\
             if msg == \"add\" then s.n += 1 end\n\
             if msg == \"run\" then return s, {{ herdr.run(\"new_window\") }} end\n\
             if msg == \"later\" then return s, {{ herdr.timer(1, \"run\") }} end\n\
             return s\n\
           end,\n\
           view = function(s, app) return ui.text(\"{text}\" .. s.n .. \"/\" .. #app.agents.all) end,\n\
         }}\n"
    )
}

fn text(node: &Node) -> &str {
    match node {
        Node::Text(text) => &text.text,
        other => panic!("expected text, got {other:?}"),
    }
}

/// Polls `poll` until it returns something, for at most ten seconds.
fn eventually<T>(mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(found) = poll() {
            return found;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}
