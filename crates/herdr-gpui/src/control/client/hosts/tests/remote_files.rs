use super::super::{super::exit, super::help, remote_files_output};
use super::text;
use crate::{
    cli::{
        CliError,
        control::{ControlCommand, parse},
    },
    control::protocol::hosts::{CopyState, RemoteCopy},
};
use std::ffi::OsString;

fn copy(path: &str, state: CopyState, auto_upload: bool) -> RemoteCopy {
    RemoteCopy {
        host: "me@devbox".into(),
        path: path.into(),
        local: format!("/cache/1{path}"),
        state,
        auto_upload,
    }
}

fn command(args: &[&str]) -> Result<ControlCommand, CliError> {
    let rest = args[1..].iter().map(OsString::from);
    parse(OsString::from(args[0]), rest).map(|invocation| invocation.command)
}

#[test]
fn remote_files_parses_with_or_without_list() {
    assert_eq!(
        command(&["remote-files"]).unwrap(),
        ControlCommand::RemoteFiles
    );
    assert_eq!(
        command(&["remote-files", "list", "--json"]).unwrap(),
        ControlCommand::RemoteFiles
    );
    assert!(matches!(
        command(&["remote-files", "upload"]).unwrap_err(),
        CliError::UnknownCommand {
            family: "remote-files",
            ..
        }
    ));
    assert!(command(&["remote-files", "list", "extra"]).is_err());
}

#[test]
fn remote_files_read_one_line_per_copy() {
    let copies = [
        copy("/srv/a.md", CopyState::Conflict, false),
        copy("/srv/b.md", CopyState::Synced, true),
    ];
    let output = remote_files_output(&copies, 0).unwrap();
    assert_eq!(
        output.text,
        "conflict  me@devbox:/srv/a.md  -> /cache/1/srv/a.md\n\
         synced (auto-upload)  me@devbox:/srv/b.md  -> /cache/1/srv/b.md"
    );
    assert_eq!(output.status, 0);
    assert_eq!(output.json["omitted"], 0);
    assert_eq!(output.json["copies"][0]["state"], "conflict");
    assert_eq!(output.json["copies"][1]["auto_upload"], true);
    assert_eq!(output.json["copies"][1]["local"], "/cache/1/srv/b.md");
    let states = [
        (CopyState::Edited, "edited"),
        (CopyState::Asking, "asking"),
        (CopyState::Uploading, "uploading"),
        (CopyState::Declined, "declined"),
        (CopyState::Refetching, "refetching"),
    ];
    for (state, name) in states {
        let line = text::remote_files(&[copy("/x", state, false)], 0);
        assert!(line.starts_with(&format!("{name}  ")), "{line}");
    }
}

#[test]
fn no_remote_files_is_nothing_yet() {
    let output = remote_files_output(&[], 0).unwrap();
    assert_eq!(output.text, "No remote file copies.");
    assert_eq!(output.status, exit::NOTHING);
    assert_eq!(
        output.json,
        serde_json::json!({ "copies": [], "omitted": 0 })
    );
    // Copies that did not fit are still copies.
    let cut = remote_files_output(&[copy("/srv/a.md", CopyState::Synced, false)], 3).unwrap();
    assert_eq!(cut.status, 0);
    assert!(
        cut.text.ends_with("… and 3 more copies that did not fit"),
        "{}",
        cut.text
    );
}

#[test]
fn remote_files_has_its_help() {
    let usage = help::usage(Some("remote-files"));
    assert!(
        usage.starts_with("herdr-gpui remote-files [list]\n"),
        "{usage}"
    );
    assert!(usage.contains("Uploads are confirmed in the\n    app."));
    assert!(help::usage(None).contains("  remote-files  "));
}
