#![allow(clippy::unwrap_used)]
use super::*;
use crate::cloud::{
    CloudProvider,
    image::{Checkout, builds, catalog::tests::isolated},
};
use std::sync::{Mutex, atomic::AtomicBool};

#[test]
fn images_are_tagged_by_project_and_key() {
    let project = Project {
        id: "0123456789abcdef".into(),
        label: "Sheprd".into(),
    };
    let key = "a".repeat(64);
    assert_eq!(
        name(&project, &key),
        "herdr-herdr-gpui-01234567:aaaaaaaaaaaa"
    );
    assert!(crate::cloud::names::image_reference(&name(&project, &key)));
}

#[test]
fn an_image_a_container_still_uses_is_kept_not_forced() {
    assert!(in_use(
        "Error response from daemon: conflict: unable to remove repository reference \"herdr-app:k\" (must force) - container 1f2 is using its referenced image 3e4"
    ));
    assert!(in_use(
        "Error: image used by 9a8: image is in use by a container: consider listing external containers and force-removing image"
    ));
    assert!(!in_use("Error: no such image"));
}

/// Opt-in: builds a real image on the engine named by
/// `HERDR_TEST_CONTAINER_ENGINE` from a recipe in a temporary checkout,
/// reuses it, and deletes it. Run with `cargo test -p sheprd-gpui
/// container::image::tests -- --ignored`.
#[test]
#[ignore = "needs a running Docker or Podman engine and network access"]
fn a_recipe_builds_once_with_herdr_and_its_files() {
    let engine = std::env::var("HERDR_TEST_CONTAINER_ENGINE")
        .ok()
        .and_then(|key| Engine::from_key(&key))
        .unwrap_or_else(|| panic!("set HERDR_TEST_CONTAINER_ENGINE to docker or podman"));
    let _catalog = isolated();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "-q"]);
    let sandbox = root.join(".herdr/sandbox");
    fs::create_dir_all(&sandbox).unwrap();
    fs::write(
        sandbox.join("Dockerfile"),
        "FROM buildpack-deps:bookworm\nCOPY hello.txt /hello.txt\nUSER nobody\n",
    )
    .unwrap();
    fs::write(sandbox.join("hello.txt"), "from the recipe\n").unwrap();
    let host = crate::teleport::Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
    let path = root.to_str().unwrap();
    let checkout = Checkout {
        host: &host,
        endpoint_id: "local",
        path,
        label: "image-live-test",
    };
    let cancelled = AtomicBool::new(false);
    let recipe = crate::cloud::image::read(&checkout, &cancelled)
        .unwrap()
        .unwrap();
    assert_eq!(recipe.files.len(), 1);
    let steps = Mutex::new(Vec::new());
    let reference = builds::ensure(
        CloudProvider::Container,
        &SUPPORT,
        &recipe,
        &|| false,
        &|progress| {
            steps.lock().unwrap().push(progress.clone());
        },
    )
    .unwrap();
    let result = (|| -> crate::Result<()> {
        assert!(
            steps
                .lock()
                .unwrap()
                .iter()
                .any(|progress| progress.step.is_some()),
            "the build reported numbered steps"
        );
        let run = |command: &str| {
            engine.run(
                "run",
                ["run", "--rm", &reference, "/bin/sh", "-c", command],
                Duration::from_secs(60),
                &|| false,
            )
        };
        assert_eq!(run("cat /hello.txt")?, "from the recipe");
        assert!(run("/usr/local/bin/herdr --version")?.contains("herdr"));
        assert_eq!(run("id -un")?, "nobody", "the recipe's user is restored");
        // A container made from it, as a dispatch makes one, has Herdr
        // already: no installer runs.
        let settings = crate::container::Settings {
            engine: Some(engine),
            image: reference.clone(),
            ..crate::container::Settings::resolve(&crate::config::ContainerConfig::default())?
        };
        let id = crate::container::setup::random_name();
        let made = super::super::machine::create(engine, &id, &settings, &|| false, &|| {})
            .and_then(|()| {
                Ok(crate::cloud::install::installed(
                    super::super::machine::exec(engine, &id)?,
                    &|| false,
                )?)
            });
        super::super::machine::delete(engine, &id, &|| false)?;
        assert!(made?, "Herdr is found where the bridge looks");
        // Unchanged: reused without building.
        let again = Mutex::new(0);
        let reused = builds::ensure(
            CloudProvider::Container,
            &SUPPORT,
            &recipe,
            &|| false,
            &|_| {
                *again.lock().unwrap() += 1;
            },
        )?;
        assert_eq!(reused, reference);
        assert_eq!(*again.lock().unwrap(), 0);
        Ok(())
    })();
    let project = recipe.project.id.clone();
    builds::reset(CloudProvider::Container, &SUPPORT, &project).unwrap();
    assert_eq!(
        find(&reference, &recipe, &|| false).unwrap(),
        None,
        "reset deleted it"
    );
    result.unwrap();
}
