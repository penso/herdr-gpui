use super::*;
use remote::{Write, apply_script, parse_collected};

#[test]
fn a_collect_reply_is_read_section_by_section() {
    let catalog = r#"{"version":1,"ssh":[]}"#;
    let reply = format!(
        "herdr-sync:1\nkey ssh-ed25519 AAAA x\nsum 123 45\ncatalog {}\n{catalog}\nstate missing\n",
        catalog.len()
    );
    let collected = parse_collected(reply.as_bytes()).unwrap();
    assert_eq!(collected.sum, "123 45");
    assert!(collected.catalog.ssh.is_empty());
    assert!(collected.state.is_none());
}

#[test]
fn malformed_replies_are_refused() {
    for reply in [
        "",
        "herdr-sync:2\n",
        "herdr-sync:1\nkey ssh-ed25519 AAAA\nsum $(x)\ncatalog missing\nstate missing\n",
        "herdr-sync:1\nkey ssh-ed25519 AAAA\nsum missing\ncatalog 99\n{}\nstate missing\n",
        "herdr-sync:1\nkey ssh-ed25519 AAAA\nsum missing\ncatalog missing\n",
    ] {
        assert!(
            matches!(parse_collected(reply.as_bytes()), Err(Error::Reply)),
            "{reply:?}"
        );
    }
}

#[test]
fn a_state_that_cannot_be_read_counts_as_none() {
    let reply = "herdr-sync:1\nkey ssh-ed25519 AAAA\nsum missing\ncatalog missing\nstate 2\n{}\n";
    assert!(parse_collected(reply.as_bytes()).unwrap().state.is_none());
}

#[test]
fn the_write_quotes_every_value_it_carries() {
    let script = apply_script(&Write {
        sum: "1 2",
        keys: &["ssh-ed25519 AAAA herdr-sync:a".to_owned()],
        catalog: None,
        state: Some("{\"label\": \"it's\"}"),
    });
    assert!(script.contains("!= '1 2' ]"));
    assert!(script.contains("set -- 'ssh-ed25519 AAAA herdr-sync:a'"));
    assert!(script.contains(r#"printf '%s' '{"label": "it'\''s"}'"#));
    assert!(!script.contains("cat >\"$tmp\""));
}

/// The scripts as the remote shell runs them, here, in a private home.
#[cfg(unix)]
mod shell {
    use super::*;
    use crate::script::{ScriptHost, ScriptLimits, run_script};
    use std::{sync::atomic::AtomicBool, time::Duration};

    fn run(home: &Path, script: &str, input: &[u8]) -> Vec<u8> {
        let script = format!(
            "HOME={}\nexport HOME\nunset XDG_STATE_HOME\n{script}",
            crate::shell_quote(home.to_str().unwrap())
        );
        let mut output = Vec::new();
        run_script(
            ScriptHost::Local,
            &script,
            input,
            &mut output,
            ScriptLimits {
                output: 1 << 20,
                idle: Duration::from_secs(30),
            },
            &AtomicBool::new(false),
        )
        .unwrap();
        output
    }

    fn apply(home: &Path, write: &Write) -> String {
        let script = format!("{}{}", remote::PATHS, apply_script(write));
        String::from_utf8(run(home, &script, write.catalog.unwrap_or_default())).unwrap()
    }

    #[test]
    fn collect_then_apply_on_a_fresh_home() {
        let home = tempfile::tempdir().unwrap();
        let collect = format!("{}{}", remote::PATHS, remote::COLLECT);
        let first = parse_collected(&run(home.path(), &collect, &[])).unwrap();
        assert_eq!(first.sum, "missing");
        // The key is created once and kept.
        let again = parse_collected(&run(home.path(), &collect, &[])).unwrap();
        assert_eq!(again.key, first.key);

        let mut catalog = first.catalog.clone();
        catalog.ssh.push(host(1, "me@laptop"));
        let bytes = catalog::encode_catalog(&catalog).unwrap();
        let keys = [key(1).line("laptop")];
        let state_json = encode_state(&state(2, "me@nas")).unwrap();
        let write = Write {
            sum: &first.sum,
            keys: &keys,
            catalog: Some(&bytes),
            state: Some(&state_json),
        };
        assert_eq!(apply(home.path(), &write), "herdr-sync:done 1\n");
        let after = parse_collected(&run(home.path(), &collect, &[])).unwrap();
        assert_eq!(ids(&after.catalog.ssh), [id(1)]);
        assert_eq!(after.state, Some(state(2, "me@nas")));
        let authorized = fs::read_to_string(home.path().join(".ssh/authorized_keys")).unwrap();
        assert_eq!(authorized, format!("{}\n", keys[0]));

        // The catalog the first read saw is gone: nothing is written.
        assert_eq!(apply(home.path(), &write), "herdr-sync:changed\n");
        // Against the current one, a key already there is not added twice.
        let write = Write {
            sum: &after.sum,
            catalog: None,
            state: None,
            ..write
        };
        assert_eq!(apply(home.path(), &write), "herdr-sync:done 0\n");
    }

    #[test]
    fn a_write_waits_its_turn_behind_another_sync() {
        let home = tempfile::tempdir().unwrap();
        let write = Write {
            sum: "missing",
            keys: &[],
            catalog: None,
            state: None,
        };
        let lock = home
            .path()
            .join(".local/state/herdr/client/.machine-sync.lock");
        fs::create_dir_all(&lock).unwrap();
        assert_eq!(apply(home.path(), &write), "herdr-sync:changed\n");
        fs::remove_dir(&lock).unwrap();
        assert_eq!(apply(home.path(), &write), "herdr-sync:done 0\n");
        // Its own lock goes when it finishes.
        assert!(!lock.exists());
    }

    fn ids(hosts: &[SavedHost]) -> Vec<String> {
        hosts.iter().map(|host| host.id.clone()).collect()
    }
}
