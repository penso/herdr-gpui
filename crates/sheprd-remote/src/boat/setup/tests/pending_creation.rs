use super::*;
use crate::cloud::ProviderError;
use crate::cloud::tests::server::serve;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn exhausted_retries_survive_restart_and_cannot_create_a_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let persisted = dir.path().to_path_buf();
    let attempts = AtomicUsize::new(0);
    let (base, log) = serve(move |request| {
        if request.method == "POST" {
            let record = std::fs::read_dir(&persisted)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| path.extension().is_some_and(|ext| ext == "json"))
                .unwrap();
            let record: serde_json::Value =
                serde_json::from_slice(&std::fs::read(record).unwrap()).unwrap();
            assert!(request.headers.contains(record["key"].as_str().unwrap()));
            let payload: serde_json::Value = serde_json::from_str(&request.body).unwrap();
            assert_eq!(record["payload"], payload);
            if attempts.fetch_add(1, Ordering::SeqCst) < 4 {
                return (503, "{}".into());
            }
        }
        (
            200,
            serde_json::json!({"sandbox": {
                "id": "bx_23456789", "name": "original", "state": "ready"
            }})
            .to_string(),
        )
    });
    let mut settings = crate::boat::tests::settings();
    settings.base = base;
    let key = SecretString::from("boat_fixture");
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let error = finish_setup(
        &request,
        dir.path(),
        Target::Create("original".into()),
        &|| false,
        &|_| {},
        |_| panic!("no confirmed ID yet"),
        |_| panic!("no confirmed ID yet"),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        crate::Error::Provider(ProviderError::Boat(
            super::super::super::Error::PendingCreation(_)
        ))
    ));
    // A new invocation has no in-memory state, and the account's creation
    // options and proposed name have changed in the meantime.
    settings.machine_type = Some("large".into());
    settings.ttl_seconds = Some(7200);
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let mut saved = Vec::new();
    let device = finish_setup(
        &request,
        dir.path(),
        Target::Create("replacement".into()),
        &|| false,
        &|_| {},
        |device| {
            saved.push(device);
            Ok(())
        },
        |_| Ok(()),
    )
    .unwrap();
    assert_eq!(device.label, "original");
    assert_eq!(saved.len(), 2);
    assert!(saved.iter().all(|device| device.id == "bx_23456789"));
    let log = log.lock().unwrap();
    let posts: Vec<_> = log
        .iter()
        .filter(|request| request.method == "POST")
        .collect();
    assert_eq!(posts.len(), 5);
    let header = |text: &str| {
        text.lines()
            .find(|line| line.to_ascii_lowercase().starts_with("idempotency-key:"))
            .unwrap()
            .to_owned()
    };
    for post in &posts[1..] {
        assert_eq!(header(&post.headers), header(&posts[0].headers));
        assert_eq!(post.body, posts[0].body);
    }
    // The confirmed catalog save releases the intent, so an intentional new
    // creation is possible only after recovery has completed.
    let journal = Journal::open(dir.path(), &settings).unwrap();
    let next = journal.begin(&settings, "next").unwrap();
    assert_eq!(next.name, "next");
    assert!(!header(&posts[0].headers).contains(&next.key));
}
