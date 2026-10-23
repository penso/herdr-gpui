#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::tabs::TabKind;

mod items;

#[test]
fn tabs_are_listed_one_per_line() {
    let tabs = [
        TabInfo {
            id: 3,
            kind: TabKind::Web,
            title: "Docs".into(),
            location: Some("https://a.test/".into()),
            workspace: "w_1".into(),
            host: "local:/tmp/herdr.sock".into(),
            opened_by: Some("w_1:p2".into()),
        },
        TabInfo {
            id: 4,
            kind: TabKind::Devices,
            title: "Devices".into(),
            location: None,
            workspace: "w_2".into(),
            host: "local:/tmp/herdr.sock".into(),
            opened_by: None,
        },
    ];
    assert_eq!(
        describe(&tabs),
        "3\tweb\tDocs\thttps://a.test/\t(workspace w_1, opened by w_1:p2)\n\
         4\tdevices\tDevices\t(workspace w_2)"
    );
    assert_eq!(describe(&[]), "");
}

#[test]
fn files_and_folders_are_resolved_before_they_are_sent() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("main.rs");
    std::fs::write(&file, "fn main() {}\n").unwrap();
    let resolved = local_file(&file).unwrap();
    assert!(Path::new(&resolved).is_absolute());
    assert_eq!(Path::new(&resolved), file.canonicalize().unwrap());
    assert!(matches!(
        local_file(folder.path()),
        Err(crate::Error::Control(crate::control::Error::Rejected {
            code: ErrorCode::InvalidRequest,
            ..
        }))
    ));
    assert!(matches!(
        local_file(&folder.path().join("missing.rs")),
        Err(crate::Error::Control(crate::control::Error::Rejected {
            code: ErrorCode::NotFound,
            ..
        }))
    ));
    assert_eq!(
        Path::new(&local_folder(folder.path()).unwrap()),
        folder.path().canonicalize().unwrap()
    );
    assert!(local_folder(&file).is_err());
}

#[test]
fn opened_tabs_say_where() {
    let output = opened(
        "/src/main.rs",
        Response::Tab {
            reply: Reply::Opened {
                id: 7,
                workspace: "w_1".into(),
            },
        },
    )
    .unwrap();
    assert_eq!(output.text, "Opened /src/main.rs in tab 7 of workspace w_1");
    assert_eq!(output.json["id"], 7);
    assert!(
        opened(
            "x",
            Response::Tab {
                reply: Reply::Text { text: "x".into() }
            }
        )
        .is_err()
    );
}
