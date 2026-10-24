use super::*;

#[test]
fn a_distribution_is_snapshotted_only_from_windows() {
    let wsl = ConnectTarget::Wsl {
        distro: "Ubuntu".into(),
        session: "default".into(),
    };
    assert_eq!(
        host_for(&wsl, false),
        cfg!(windows).then(|| Host::Wsl("Ubuntu".into()))
    );
}

#[test]
fn a_name_that_is_not_a_distribution_runs_nothing() {
    let checkout = Checkout {
        host: Host::Wsl("a;b".into()),
        ..feature()
    };
    let error = run(&checkout, "true").unwrap_err();
    assert!(
        matches!(
            &error,
            crate::Error::Checkpoint(Error::Script(herdr_client::Error::Wsl(
                herdr_client::wsl::Error::InvalidDistro
            )))
        ),
        "{error:?}"
    );
}

/// Takes and lists a checkpoint of a scratch repository in a real
/// distribution's `/tmp`, then removes it:
/// `HERDR_TEST_WSL_DISTRO=Ubuntu-22.04 cargo test -p herdr-gpui
/// live_wsl_checkpoints -- --ignored --nocapture`. Needs `git` there.
#[cfg(windows)]
#[test]
#[ignore = "runs Git in a real WSL distribution"]
fn live_wsl_checkpoints() {
    use super::super::direct;
    let distro = std::env::var("HERDR_TEST_WSL_DISTRO").expect("HERDR_TEST_WSL_DISTRO");
    let setup = "set -e
d=$(mktemp -d /tmp/herdr-checkpoint.XXXXXX)
cd \"$d\"
git init -q -b feature
git -c user.name=t -c user.email=t@t commit -q --allow-empty -m init
echo changed > file
printf '%s' \"$d/.git\"";
    let mut output = Vec::new();
    run_script(
        ScriptHost::Wsl(&distro),
        setup,
        std::io::empty(),
        &mut output,
        LIMITS,
        &AtomicBool::new(false),
    )
    .unwrap();
    let repo = String::from_utf8(output).unwrap();
    let checkout = Checkout::new(Host::Wsl(distro.clone()), &repo, Some("feature")).unwrap();
    let taken = direct::create(&checkout, "live", direct::here);
    let listed = direct::list(&checkout, direct::here);
    let cleanup = format!(
        "rm -rf -- {}",
        herdr_client::shell_quote(repo.trim_end_matches("/.git"))
    );
    run_script(
        ScriptHost::Wsl(&distro),
        &cleanup,
        std::io::empty(),
        std::io::sink(),
        LIMITS,
        &AtomicBool::new(false),
    )
    .unwrap();
    let id = taken.unwrap().expect("a change to checkpoint");
    let listed = listed.unwrap();
    println!("{listed:?}");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    assert_eq!(listed[0].label, "live");
}
