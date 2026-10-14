use super::*;

fn connected(targets: &[&str]) -> Vec<(String, SshTarget)> {
    targets
        .iter()
        .map(|target| (format!("label-{target}"), (*target).into()))
        .collect()
}

fn history(enabled: bool) -> anyhow::Result<RemotePaneHistory> {
    Ok(RemotePaneHistory::parse_text(Some(format!(
        "[experimental]\npane_history = {enabled}\n"
    )))?)
}

#[test]
fn failures_show_their_ssh_cause_once() {
    let error: crate::Error =
        crate::herdr_settings::Error::Remote(herdr_client::Error::ScriptTimeout).into();
    assert_eq!(
        describe(&error),
        "could not reach the host's Herdr config: host script made no progress before its deadline"
    );
}

#[test]
fn sync_loads_new_hosts_once_and_drops_disconnected_ones() {
    let mut remote = RemoteHistory::default();
    let jobs = remote.sync(connected(&["a", "b", "a"]), false);
    assert_eq!(
        jobs.iter()
            .map(|job| job.target.target.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(remote.sync(connected(&["a", "b"]), false).is_empty());

    let dropped = jobs[1].cancel.clone();
    assert!(remote.sync(connected(&["a"]), false).is_empty());
    assert!(
        dropped.load(Ordering::Acquire),
        "a disconnected host stops its SSH command"
    );
    assert!(!jobs[0].cancel.load(Ordering::Acquire));
    assert_eq!(remote.hosts().len(), 1);
}

#[test]
fn late_results_cannot_overwrite_a_newer_read() -> anyhow::Result<()> {
    let mut remote = RemoteHistory::default();
    let first = remote.sync(connected(&["a"]), false).remove(0);
    let second = remote.sync(connected(&["a"]), true).remove(0);
    assert!(first.cancel.load(Ordering::Acquire));
    assert!(!remote.finish(&"a".into(), first.generation, Ok(history(true)?)));
    assert!(matches!(remote.hosts()[0].state, HostState::Loading));
    assert!(remote.finish(&"a".into(), second.generation, Ok(history(false)?)));
    assert!(matches!(
        &remote.hosts()[0].state,
        HostState::Ready(history) if !history.enabled
    ));
    Ok(())
}

#[test]
fn saves_start_only_from_a_settled_read_and_survive_reload() -> anyhow::Result<()> {
    let mut remote = RemoteHistory::default();
    let load = remote.sync(connected(&["a"]), false).remove(0);
    assert!(remote.begin_save(&"a".into()).is_none(), "still loading");
    assert!(remote.finish(&"a".into(), load.generation, Ok(history(false)?)));

    let Some((from, save)) = remote.begin_save(&"a".into()) else {
        anyhow::bail!("a ready host saves");
    };
    assert!(!from.enabled);
    assert!(
        remote.begin_save(&"a".into()).is_none(),
        "one write at a time"
    );
    // A reload while the write runs leaves it alone.
    assert!(remote.sync(connected(&["a"]), true).is_empty());
    assert!(!save.cancel.load(Ordering::Acquire));
    assert!(!remote.finish(&"a".into(), load.generation, Ok(history(false)?)));
    assert!(remote.finish(
        &"a".into(),
        save.generation,
        Err(crate::herdr_settings::Error::RemoteConflict.into())
    ));
    assert!(matches!(
        &remote.hosts()[0].state,
        HostState::Failed(text) if text == "the host's Herdr config changed; reload before saving"
    ));
    assert!(
        remote.begin_save(&"a".into()).is_none(),
        "a failed host reloads first"
    );
    assert_eq!(remote.sync(connected(&["a"]), true).len(), 1);
    Ok(())
}
