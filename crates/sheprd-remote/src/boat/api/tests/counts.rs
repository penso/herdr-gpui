use super::*;
use crate::cloud::tests::server::serve;
use secrecy::SecretString;

#[test]
fn full_pages_are_lower_bounds_and_short_pages_have_exact_counts() {
    for (count, expected) in [
        (0, SandboxCount::Exact(0)),
        (1, SandboxCount::Exact(1)),
        (99, SandboxCount::Exact(99)),
        (100, SandboxCount::AtLeast(100)),
    ] {
        let (base, _) = serve(move |request| {
            assert_eq!(request.method, "GET");
            assert_eq!(request.target, "/sandboxes?limit=100");
            let sandboxes = vec![
                serde_json::json!({
                    "id": "bx_23456789", "name": "fixture", "state": "ready"
                });
                count
            ];
            (200, serde_json::json!({"sandboxes": sandboxes}).to_string())
        });
        let mut settings = crate::boat::tests::settings();
        settings.base = base;
        let key = SecretString::from("boat_fixture");
        assert_eq!(
            Request {
                settings: &settings,
                key: &key
            }
            .sandbox_count()
            .unwrap(),
            expected
        );
    }
}
