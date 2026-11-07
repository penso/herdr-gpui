#![allow(clippy::unwrap_used)]
use super::*;

fn snapshot(state: &str, reason: Option<&str>) -> Snapshot {
    Snapshot {
        state: state.into(),
        error_reason: reason.map(Into::into),
        build_info: None,
    }
}

#[test]
fn snapshot_states_map_onto_a_build() {
    assert_eq!(snapshot("active", None).readiness(), Readiness::Ready);
    assert_eq!(
        snapshot("build_failed", Some("RUN exited 1")).readiness(),
        Readiness::Failed("RUN exited 1".into())
    );
    assert_eq!(
        snapshot("error", Some("")).readiness(),
        Readiness::Failed("error".into())
    );
    assert_eq!(
        snapshot("pending_build", None).readiness(),
        Readiness::Pending("pending build".into())
    );
}

#[test]
fn snapshot_names_are_safe_in_paths() {
    let project = Project {
        id: "0123456789abcdef".into(),
        label: "app".into(),
    };
    let name = name(&project, &"b".repeat(64));
    assert_eq!(name, "herdr-app-01234567-bbbbbbbbbbbb");
    assert!(segment(&name).is_ok());
    assert!(segment("../x").is_err());
    assert!(segment("a/b").is_err());
    assert!(segment("").is_err());
}

#[test]
fn only_a_snapshot_built_from_the_recipe_is_trusted() {
    let recipe = Recipe {
        project: Project {
            id: "0123456789abcdef".into(),
            label: "app".into(),
        },
        origin: crate::cloud::image::catalog::Origin {
            endpoint_id: "local".into(),
            path: "/app".into(),
        },
        dockerfile: "FROM debian\n".into(),
        files: Vec::new(),
    };
    let parsed = |json: &str| serde_json::from_str::<Snapshot>(json).unwrap();
    assert!(
        parsed(r#"{"state":"active","buildInfo":{"dockerfileContent":"FROM debian\n"}}"#)
            .built_from(&recipe)
    );
    // Same name, other content: someone else's snapshot.
    assert!(
        !parsed(r#"{"state":"active","buildInfo":{"dockerfileContent":"FROM evil\n"}}"#)
            .built_from(&recipe)
    );
    // Same Dockerfile, but with files a bind mount could read.
    assert!(!parsed(
        r#"{"state":"active","buildInfo":{"dockerfileContent":"FROM debian\n","contextHashes":["abc"]}}"#
    )
    .built_from(&recipe));
    // Made from an image rather than built.
    assert!(!parsed(r#"{"state":"active","imageName":"evil:latest"}"#).built_from(&recipe));
    assert!(!parsed(r#"{"state":"active","buildInfo":null}"#).built_from(&recipe));
}
