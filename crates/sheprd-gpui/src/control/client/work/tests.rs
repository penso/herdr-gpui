#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use std::path::Path;

#[cfg(unix)]
mod checkout;

#[test]
fn notes_name_their_repository_by_its_directory() {
    assert_eq!(repository_name("/src/app/.git"), "app");
    assert_eq!(repository_name("/src/app/.git/"), "app");
    assert_eq!(repository_name("/srv/repos/tool.git"), "tool.git");
    assert_eq!(repository_name("/"), "/");
    assert_eq!(
        describe_note(&NoteInfo {
            host: "local".into(),
            repository: "/src/app/.git".into(),
            branch: "feature".into(),
            text: "waiting on review".into(),
        }),
        "feature (app, local): waiting on review"
    );
}

#[test]
fn script_listings_show_first_lines_and_whole_scripts() {
    let root = Path::new("/src/app");
    let (text, json) = scripts::listing(root, None);
    assert_eq!(text, "No .herdr/worktree.toml in /src/app");
    assert_eq!(json["present"], false);
    let config = crate::worktree_scripts::Config::parse(
        b"[scripts]\nsetup = \"\"\"\n\nnpm ci\ncp ../.env .\n\"\"\"\nrun = \"npm run dev\"\n",
    )
    .unwrap();
    let (text, json) = scripts::listing(root, Some(&config));
    assert_eq!(
        text,
        "setup   npm ci ...\nrun     npm run dev\narchive (none)"
    );
    assert_eq!(json["scripts"]["setup"], "\nnpm ci\ncp ../.env .\n");
    assert_eq!(json["scripts"]["run"], "npm run dev");
    assert_eq!(json["scripts"]["archive"], serde_json::Value::Null);
    assert_eq!(json["digest"], config.digest.as_str());
}
