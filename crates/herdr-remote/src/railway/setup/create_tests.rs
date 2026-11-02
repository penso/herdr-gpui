//! Setup against a stand-in for Railway: a headless probe is refused until a
//! session with an 80x32 terminal has run, as with the real service.
#![allow(clippy::unwrap_used)]
use super::*;
use crate::cloud::Step;
use crate::railway::{
    ssh::stand_in,
    tests::{key, tools},
};
use std::cell::RefCell;

/// Logs what each run was (`@LOG@`) and keeps whether a VM exists in `@STATE@`.
/// A terminal run (`-tt`) does what `terminal` says; headless runs are refused
/// with exit 13 until the VM exists.
fn railway(terminal: &str) -> String {
    r#"mode=headless
last=
for argument in "$@"; do
    [ "$argument" = -tt ] && mode=terminal
    last=$argument
done
if [ $mode = terminal ]; then
@TERMINAL@
fi
if [ ! -e '@STATE@/vm' ]; then
    echo refused >> '@LOG@'
    echo '{"status":"refused","description":"Headless terminals are not allowed"}'
    exit 13
fi
case "$last" in
    *'exit 3'*) echo probe >> '@LOG@' ;;
    railway.new) echo reach >> '@LOG@' ;;
    *) echo "reach with a command" >> '@LOG@' ;;
esac
exit 0
"#
    .replace("@TERMINAL@", terminal)
}

/// A terminal run that creates the VM only when it has a terminal of 80x32.
const CREATES: &str = r#"    if [ -t 0 ] && [ "$(stty size 2>/dev/null)" = "32 80" ]; then
        echo "terminal 32 80" >> '@LOG@'
        : > '@STATE@/vm'
        exit 0
    fi
    echo "terminal wrong" >> '@LOG@'
    exit 13"#;

struct Fixture {
    dir: tempfile::TempDir,
    _keys: tempfile::TempDir,
    key: PathBuf,
    id: String,
}

fn fixture() -> Option<Fixture> {
    tools().then(|| {
        let keys = tempfile::tempdir().unwrap();
        let (key, id) = key(keys.path(), "theirs");
        Fixture {
            dir: tempfile::tempdir().unwrap(),
            _keys: keys,
            key,
            id,
        }
    })
}

/// Set up `key` under `name`; `saved` is the device list, which a success adds to.
fn run(
    fixture: &Fixture,
    (key, name): (&Path, &str),
    steps: &RefCell<Vec<Step>>,
    saved: &RefCell<Vec<SavedDevice>>,
) -> Result<SavedDevice> {
    add_device_in(
        fixture.dir.path(),
        AddRequest {
            key: key.to_owned(),
            name: name.to_owned(),
            install: false,
        },
        &|| false,
        &|step| steps.borrow_mut().push(step),
        || Ok(saved.borrow().clone()),
        |device| {
            saved.borrow_mut().push(device);
            Ok(())
        },
    )
}

#[test]
fn a_key_with_no_vm_gets_one_from_an_80x32_terminal_once_then_only_headless_runs() {
    let Some(fixture) = fixture() else { return };
    let ssh = stand_in::install_script(&railway(CREATES));
    let (steps, saved) = (RefCell::new(Vec::new()), RefCell::new(Vec::new()));
    let device = run(&fixture, (&fixture.key, "first"), &steps, &saved).unwrap();
    assert_eq!(
        ssh.runs(),
        ["refused", "terminal 32 80", "reach", "probe"],
        "a bare probe, one terminal session, a bare probe, then the installer's probe"
    );
    assert_eq!(
        *steps.borrow(),
        [Step::Connecting, Step::Creating, Step::Connecting]
    );
    // The device is the key's: its ID names the key, its account is the path.
    assert_eq!(device.id, fixture.id);
    assert_eq!(device.account, fixture.key.display().to_string());
    assert_eq!(
        (device.label.as_str(), device.machine.as_str()),
        ("first", "first")
    );

    // The VM exists now: setting up again never opens a terminal, and the
    // device keeps its name.
    steps.borrow_mut().clear();
    let again = run(&fixture, (&fixture.key, "second"), &steps, &saved).unwrap();
    assert_eq!(ssh.runs()[4..], ["reach", "probe"]);
    assert_eq!(*steps.borrow(), [Step::Connecting]);
    assert_eq!(
        (again.label.as_str(), again.machine.as_str()),
        ("first", "first")
    );
}

#[test]
fn refusals_report_railways_words_without_its_links_and_never_loop_or_save() {
    let Some(fixture) = fixture() else { return };
    let cases: [(&str, &str, &[&str], &str); 4] = [
        // The daily limit comes back on the probe: same exit status as the
        // headless refusal, but a terminal session would not change it.
        (
            "exit 0",
            "limit on the probe",
            &["refused"],
            "Anonymous visitors are limited",
        ),
        // The same refusal from the terminal session itself.
        (
            "    echo 'Anonymous visitors are limited. Sign up: https://railway.com/signup?code=secret'\n    exit 13",
            "limit on the session",
            &["refused"],
            "Anonymous visitors are limited",
        ),
        // A session that ends without a VM is not retried.
        (
            "    exit 0",
            "no vm",
            &["refused", "refused"],
            "without creating",
        ),
        (
            "    echo 'down: https://railway.com/claim?code=secret'\n    exit 1",
            "failure",
            &["refused"],
            "down",
        ),
    ];
    for (terminal, name, runs, expected) in cases {
        let script = if name == "limit on the probe" {
            r#"echo refused >> '@LOG@'
echo 'Anonymous visitors are limited. Sign up: https://railway.com/signup?code=secret'
exit 13
"#
            .to_owned()
        } else {
            railway(terminal)
        };
        let ssh = stand_in::install_script(&script);
        let (steps, saved) = (RefCell::new(Vec::new()), RefCell::new(Vec::new()));
        let error = run(&fixture, (&fixture.key, "n"), &steps, &saved)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{name}: {error}");
        assert!(!error.contains("secret"), "{name}: {error}");
        assert_eq!(ssh.runs(), runs, "{name}");
        assert!(saved.borrow().is_empty(), "{name}");
    }
}

#[test]
fn a_key_or_device_list_that_cannot_be_used_costs_no_vm() {
    let Some(fixture) = fixture() else { return };
    let ssh = stand_in::install_script(&railway(CREATES));
    let (steps, saved) = (RefCell::new(Vec::new()), RefCell::new(Vec::new()));
    let junk = fixture.dir.path().join("junk");
    std::fs::write(&junk, "not a key\n").unwrap();
    for bad in [
        junk.as_path(),
        Path::new("relative"),
        &fixture.dir.path().join("absent"),
    ] {
        assert!(
            run(&fixture, (bad, "n"), &steps, &saved).is_err(),
            "{bad:?}"
        );
    }
    let unreadable = add_device_in(
        fixture.dir.path(),
        AddRequest {
            key: fixture.key.clone(),
            name: "n".into(),
            install: false,
        },
        &|| false,
        &|_| {},
        || Err(cloud::Error::StateDirectory),
        |_| Ok(()),
    );
    assert!(matches!(unreadable, Err(Error::Cloud(_))));
    assert!(ssh.runs().is_empty(), "nothing reached Railway");
}
