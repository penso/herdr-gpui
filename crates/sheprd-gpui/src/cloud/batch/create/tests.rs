#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::batch::{BatchKind, Phase};
use std::time::Instant;

fn creation(count: u32) -> Creation {
    Creation {
        provider: CloudProvider::ALL[0],
        count,
        spec: Spec::new(),
        setup: true,
    }
}

fn settled(registry: &Registry, id: u64) -> crate::control::batch::BatchInfo {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let batch = registry.get(id).unwrap();
        if batch.phase.settled() {
            return batch;
        }
        assert!(Instant::now() < deadline, "{batch:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn each_machine_is_named_once_made_and_ready_once_its_work_ends() {
    let registry = Registry::default();
    let wanted = creation(3);
    let pending = registry.open(BatchKind::Create, wanted.devices(), wanted);
    let (registry, id, wanted) = pending.confirm().unwrap();
    let provider = wanted.provider;
    start(&registry, id, wanted.count as usize, move |report| {
        report.step(DeviceState::Creating, "Building…".into());
        let n = report.index;
        if n == 2 {
            return Err(Error::NotOffered("Nimbus").into());
        }
        report.created(&Target {
            provider,
            id: format!("m{n}"),
            endpoint: format!("{}:m{n}", provider.key()),
            label: format!("herdr-{n}"),
        });
        report.step(DeviceState::SettingUp, "Running the setup script…".into());
        Ok(())
    });
    let batch = settled(&registry, id);
    assert_eq!(batch.phase, Phase::Done);
    let states: Vec<_> = batch.devices.iter().map(|device| device.state).collect();
    assert_eq!(
        states,
        [DeviceState::Ready, DeviceState::Ready, DeviceState::Failed]
    );
    assert_eq!(batch.devices[1].label, "herdr-1");
    assert!(batch.devices[1].endpoint.ends_with(":m1"));
    assert_eq!(batch.devices[1].step, None, "a settled device has no step");
    assert!(
        batch.devices[2].endpoint.is_empty(),
        "never made, never named"
    );
    assert_eq!(
        batch.devices[2].error.as_deref(),
        Some("Nimbus machines cannot be created from the command line.")
    );
}

#[test]
fn devices_are_numbered_before_they_exist() {
    let devices = creation(2).devices();
    assert_eq!(devices.len(), 2);
    assert!(devices[1].label.starts_with("New ") && devices[1].label.ends_with(" 2"));
    assert!(devices.iter().all(|device| device.endpoint.is_empty()));
}

#[test]
fn the_setup_script_runs_in_a_login_shell_from_home() {
    let command = setup_command(CloudProvider::ALL[0], "echo 'hi'".into());
    assert_eq!(command[0], "env");
    assert_eq!(
        command[1],
        format!("HERDR_CLOUD_PROVIDER={}", CloudProvider::ALL[0].key())
    );
    assert_eq!(command.last().unwrap(), "echo 'hi'");
    #[cfg(unix)]
    {
        let line = exec::script(&command, None).unwrap();
        let mut shell = sheprd_client::new_command("/bin/sh");
        shell.args(["-c", &line]).env("SHELL", "/bin/sh");
        let finished =
            exec::capture(shell, Duration::from_secs(10), &AtomicBool::new(false)).unwrap();
        assert_eq!(finished.code, Some(0));
        assert_eq!(finished.stdout, "hi\n");
    }
}

#[test]
fn a_failed_setup_says_how_it_ended() {
    let finished = exec::Finished {
        code: Some(2),
        stdout: (1..=8).map(|n| format!("line {n}\n")).collect(),
        stderr: "\nboom\n".into(),
        truncated: false,
    };
    let output = tail(&finished);
    assert_eq!(output, "line 4 / line 5 / line 6 / line 7 / line 8 / boom");
    assert_eq!(
        Error::Setup {
            code: Some(2),
            output
        }
        .to_string(),
        "The setup script exited 2: line 4 / line 5 / line 6 / line 7 / line 8 / boom"
    );
    assert_eq!(
        Error::Setup {
            code: None,
            output: String::new()
        }
        .to_string(),
        "A signal ended the setup script."
    );
}

#[cfg(feature = "boat")]
#[test]
fn a_boat_sandbox_saved_before_setup_failed_is_still_named() {
    let failed: crate::Error = crate::boat::Error::Setup {
        id: "sandbox-1".into(),
        source: Box::new(sheprd_remote::cloud::Error::Cancelled.into()),
    }
    .into();
    assert_eq!(saved_id(&failed), Some("sandbox-1"));
    let other: crate::Error = Error::NotOffered("Boat").into();
    assert_eq!(saved_id(&other), None);
}
