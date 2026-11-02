use super::{connected, terminal};
use crate::CoreBackend;
use herdr_backend::{Backend, Spawn};
use serde_json::json;
use std::{collections::BTreeMap, thread};

#[test]
fn a_spawn_sends_its_labels_and_reads_them_back() {
    let (core, mut mock) = connected();
    let backend = CoreBackend::new(core, 1000);
    let spawn = Spawn {
        argv: vec!["herdr-acp".into(), "codex".into()],
        env: vec![("HERDR_ACP_ENDPOINT".into(), "ab".repeat(16))],
        labels: BTreeMap::from([("acp".into(), "ab".repeat(16))]),
        ..Spawn::default()
    };
    let info = thread::scope(|scope| {
        let run = scope.spawn(|| backend.spawn(&spawn));
        let request = mock.request();
        assert_eq!(request["method"], "terminal.run");
        assert_eq!(request["params"]["labels"], json!({"acp": "ab".repeat(16)}));
        assert_eq!(
            request["params"]["env"],
            json!({"HERDR_ACP_ENDPOINT": "ab".repeat(16)})
        );
        let mut made = terminal("t4");
        made["labels"] = request["params"]["labels"].clone();
        mock.answer(&request, made);
        run.join().unwrap()
    })
    .unwrap();
    assert_eq!(info.labels.get("acp"), Some(&"ab".repeat(16)));
}

#[test]
fn a_spawn_without_labels_sends_none() {
    let (core, mut mock) = connected();
    let backend = CoreBackend::new(core, 1000);
    let info = thread::scope(|scope| {
        let run = scope.spawn(|| backend.spawn(&Spawn::default()));
        let request = mock.request();
        assert!(request["params"].get("labels").is_none());
        mock.answer(&request, terminal("t5"));
        run.join().unwrap()
    })
    .unwrap();
    assert!(info.labels.is_empty());
}
