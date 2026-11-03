use super::*;
use std::time::{Duration, SystemTime};

mod budgets;
mod data;
mod effects;
mod manifest;
mod pace;
mod runtime;
mod sources;
mod state;
mod tree;

const MANIFEST: &str = "name = \"demo\"\napi = 1\n";

fn files(entries: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
    entries
        .iter()
        .map(|(name, text)| ((*name).to_owned(), text.as_bytes().to_vec()))
        .collect()
}

fn sources(main: &str) -> Sources {
    Sources::new(files(&[("mod.toml", MANIFEST), ("main.luau", main)])).unwrap()
}

fn load(main: &str) -> Result<Mod> {
    Mod::load(&sources(main), Limits::default(), SystemTime::UNIX_EPOCH)
}

/// A mod whose view returns `tree`, an expression with `ui`, `state`, and
/// `app` in scope.
fn view_mod(tree: &str) -> String {
    format!(
        "local ui = require(\"@herdr/ui\")\n\
         return herdr.mod {{ view = function(state, app) return {tree} end }}\n"
    )
}

/// The tree a view draws; a view that draws nothing fails the test.
fn view(tree: &str) -> Result<Node> {
    load(&view_mod(tree))?
        .view(&State::default(), SystemTime::UNIX_EPOCH)
        .map(|node| node.unwrap())
}

fn tree_problem(tree: &str) -> (String, Problem) {
    match view(tree).unwrap_err() {
        Error::Tree { at, problem } => (at, problem),
        other => panic!("expected a tree error, got {other:?}"),
    }
}

fn text(text: &str) -> Node {
    Node::Text(Text {
        text: text.to_owned(),
        tone: Tone::Normal,
        size: Size::Normal,
        weight: Weight::Normal,
        truncate: false,
    })
}
