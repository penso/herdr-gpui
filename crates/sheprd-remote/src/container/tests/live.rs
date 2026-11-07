//! Opt-in: makes a real container on the engine named by
//! `HERDR_TEST_CONTAINER_ENGINE` (`docker` or `podman`), installs Herdr in it
//! from the network, attaches the stdio bridge, and deletes it again. Run with
//! `cargo test -p sheprd-remote --features container container::tests::live -- --ignored`.

use super::*;
use std::sync::atomic::AtomicBool;

#[test]
#[ignore = "needs a running Docker or Podman engine and network access"]
fn a_container_is_made_reached_and_deleted() {
    let engine = std::env::var("HERDR_TEST_CONTAINER_ENGINE")
        .ok()
        .and_then(|key| Engine::from_key(&key))
        .unwrap_or_else(|| panic!("set HERDR_TEST_CONTAINER_ENGINE to docker or podman"));
    let settings = Settings {
        engine: Some(engine),
        ..Settings::resolve(&ContainerConfig::default()).unwrap()
    };
    let id = setup::random_name();
    let never = || false;
    let result = (|| -> crate::Result<()> {
        machine::create(engine, &id, &settings, &never, &|| {})?;
        assert_eq!(
            machine::state(engine, &id, &never)?,
            machine::State::Running
        );
        // The agents' user owns its home.
        let owner = engine.run(
            "exec",
            [
                "exec",
                "--user",
                machine::USER,
                &id,
                "stat",
                "-c",
                "%U",
                machine::HOME,
            ],
            std::time::Duration::from_secs(30),
            &never,
        )?;
        assert_eq!(owner, machine::USER);
        // The container is found as this app's, for one left unsaved.
        assert!(machine::labelled(engine, &never)?.contains(&id));
        assert!(!crate::cloud::install::installed(
            machine::exec(engine, &id)?,
            &never
        )?);
        // One left before Herdr was installed is not saved as a device that
        // cannot connect: adding it again needs the install approved.
        lifecycle::stop(engine, &id, || machine::stop(engine, &id, &never))?;
        assert!(matches!(
            setup::adopt(engine, &id, false, &never),
            Err(crate::Error::Cloud(crate::cloud::Error::Install(_)))
        ));
        assert_eq!(
            machine::state(engine, &id, &never)?,
            machine::State::Running,
            "adding it started it"
        );
        let target = sheprd_client::ConnectTarget::Cloud {
            provider: sheprd_client::CloudProvider::Container,
            account: engine.key().into(),
            id: id.clone(),
            machine: id.clone(),
            session: "default".into(),
        };
        assert!(
            !lifecycle::suspended(&target),
            "adding it back cleared the earlier Stop"
        );
        crate::cloud::install::install(machine::exec(engine, &id)?, &never)?;
        assert!(crate::cloud::install::installed(
            machine::exec(engine, &id)?,
            &never
        )?);
        // Dispatch runs `herdr` commands before any window has connected.
        machine::start_server(engine, &id, &never)?;
        let listed = engine.run(
            "exec",
            [
                "exec",
                "--user",
                machine::USER,
                "--env",
                "HOME=/home/herdr",
                &id,
                "/bin/sh",
                "-c",
                "$HOME/.local/bin/herdr workspace list",
            ],
            std::time::Duration::from_secs(30),
            &never,
        )?;
        assert!(listed.contains("workspace_list"), "{listed}");
        // With no agent there, the auto-stop probe reads the container as idle.
        let (_, agents, _) = engine.output(
            "exec",
            [
                "exec",
                "--user",
                machine::USER,
                "--env",
                "HOME=/home/herdr",
                &id,
                "/bin/sh",
                "-c",
                super::super::idle::AGENTS,
            ],
            std::time::Duration::from_secs(30),
            &never,
        )?;
        assert!(!super::super::idle::busy_reply(&agents), "{agents:?}");
        // The bridge answers discovery, so a connection would work.
        let bridge = sheprd_client::connect_command(
            machine::exec(engine, &id)?,
            "default",
            &AtomicBool::new(false),
        )
        .map_err(Error::Bridge)?;
        drop(bridge);
        // A stopped container starts again on the next connection.
        machine::stop(engine, &id, &never)?;
        assert_eq!(
            machine::state(engine, &id, &never)?,
            machine::State::Stopped
        );
        machine::ensure_running(engine, &id, &never)?;
        assert_eq!(
            machine::state(engine, &id, &never)?,
            machine::State::Running
        );
        // Podman hands the home volume back to root on each start.
        let owner = engine.run(
            "exec",
            ["exec", &id, "stat", "-c", "%U", machine::HOME],
            std::time::Duration::from_secs(30),
            &never,
        )?;
        assert_eq!(owner, machine::USER, "the restarted container's home");
        Ok(())
    })();
    machine::delete(engine, &id, &never).unwrap();
    assert_eq!(
        machine::state(engine, &id, &never).unwrap(),
        machine::State::Missing
    );
    result.unwrap();
}

#[test]
#[ignore = "needs a running Docker or Podman engine"]
fn a_container_this_app_did_not_make_is_never_reused_or_deleted() {
    let engine = std::env::var("HERDR_TEST_CONTAINER_ENGINE")
        .ok()
        .and_then(|key| Engine::from_key(&key))
        .unwrap_or_else(|| panic!("set HERDR_TEST_CONTAINER_ENGINE to docker or podman"));
    let settings = Settings {
        engine: Some(engine),
        ..Settings::resolve(&ContainerConfig::default()).unwrap()
    };
    let never = || false;
    let id = setup::random_name();
    // The user's own container, which happens to have a generated name.
    engine
        .run(
            "run",
            [
                "run",
                "--detach",
                "--name",
                &id,
                &settings.image,
                "sleep",
                "120",
            ],
            std::time::Duration::from_secs(60),
            &never,
        )
        .unwrap();
    let created = machine::create(engine, &id, &settings, &never, &|| {});
    let listed = machine::labelled(engine, &never).unwrap();
    let deleted = machine::delete(engine, &id, &never);
    let state = machine::state(engine, &id, &never);
    let _ = engine.output(
        "rm",
        ["rm", "--force", &id],
        std::time::Duration::from_secs(30),
        &never,
    );
    assert!(matches!(created, Err(Error::NameTaken(_))), "{created:?}");
    assert!(
        !listed.contains(&id),
        "the user's own container is not ours"
    );
    assert!(matches!(deleted, Err(Error::NotOurs(_))), "{deleted:?}");
    assert_eq!(state.unwrap(), machine::State::Running, "it was left alone");
}
