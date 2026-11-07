use super::*;
use crate::teleport::{Host, Step};
use sheprd_client::{CloudProvider, ConnectTarget, ScriptLimits};
use std::time::Duration;

fn host(key: &Path, id: &str) -> Host {
    crate::teleport::host_for(&ConnectTarget::Cloud {
        provider: CloudProvider::Railway,
        account: key.to_str().unwrap().into(),
        id: id.into(),
        machine: "railway.new".into(),
        session: "work".into(),
    })
    .unwrap()
}

#[test]
fn teleport_streams_binary_data_through_the_saved_railway_identity() {
    assert!(
        tools(),
        "ssh-keygen required for Railway identity regression"
    );
    let dir = tempfile::tempdir().unwrap();
    let (saved, id) = key(dir.path(), "id");
    let ssh = stand_in::install_script(
        r#"
for arg in "$@"; do
    case "$arg" in
        IdentityFile=*) printf '%s\n' "$arg" >> '@LOG@' ;;
        -tt) exit 98 ;;
    esac
    remote=$arg
done
exec /bin/sh -c "$remote"
"#,
    );
    let host = host(&saved, &id);
    let bytes: Vec<u8> = (0..200_000).map(|n| n as u8).collect();
    assert_eq!(
        host.query(Step::Transfer, "cat", &bytes, &AtomicBool::new(false))
            .unwrap(),
        bytes
    );
    assert_eq!(
        ssh.runs(),
        [format!("IdentityFile=\"{}\"", saved.display())]
    );
    // The same runner dispatches the named daemon session, not the default.
    assert_eq!(
        host.capture(
            "herdr() { printf '%s\\n' \"$@\"; }; herdr_cli workspace list",
            &AtomicBool::new(false),
        )
        .unwrap(),
        b"--session\nwork\nworkspace\nlist\n"
    );
    let error = host
        .run(
            Step::Transfer,
            "yes",
            &[][..],
            io::sink(),
            ScriptLimits {
                output: 1024,
                idle: Duration::from_secs(5),
            },
            &AtomicBool::new(false),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        crate::teleport::Error::Script {
            step: Step::Transfer,
            source: sheprd_client::Error::ScriptOutputLimit,
        }
    ));
}

#[test]
fn teleport_refuses_a_replaced_railway_key_with_its_typed_cause() {
    assert!(
        tools(),
        "ssh-keygen required for Railway identity regression"
    );
    let dir = tempfile::tempdir().unwrap();
    let (saved, id) = key(dir.path(), "id");
    let (other, _) = key(dir.path(), "other");
    let ssh = stand_in::install_script("echo ran >> '@LOG@'; exit 99");
    fs::copy(&other, &saved).unwrap();
    fs::copy(keys::public_path(&other), keys::public_path(&saved)).unwrap();
    let error = host(&saved, &id)
        .query(Step::Review, "true", &[], &AtomicBool::new(false))
        .unwrap_err();
    assert!(std::error::Error::source(&error).is_some());
    let crate::teleport::Error::Script {
        step: Step::Review,
        source: sheprd_client::Error::ScriptConnect(error),
    } = &error
    else {
        panic!("{error:?}");
    };
    assert!(matches!(
        error.get_ref().and_then(|e| e.downcast_ref::<Error>()),
        Some(Error::KeyChanged(_))
    ));
    assert!(ssh.runs().is_empty());
}
