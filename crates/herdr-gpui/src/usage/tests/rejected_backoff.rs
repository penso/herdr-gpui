use super::*;
use crate::usage::{Host, Message, REFRESH, REJECTED_LIMIT, Usage};
use std::time::Instant;

/// One read of `host` at `now` in which `claude` is rejected and `codex`
/// answers, unless `claude` is held back and so never asked.
fn read_once(usage: &mut Usage, host: &Host, now: Instant) {
    let (claude, codex) = (provider("claude"), provider("codex"));
    begin(usage, host, now);
    if !usage.current().unwrap().held.contains(&claude) {
        usage.apply(
            Message::Reading(
                host.clone(),
                claude,
                Err(Error::Usage(usage::Error::Rejected)),
            ),
            now,
        );
    }
    usage.apply(
        Message::Reading(host.clone(), codex, Ok(report(codex, 5.))),
        now,
    );
    usage.apply(Message::Done(host.clone(), Ok(())), now);
}

fn again(usage: &Usage, provider: Provider) -> Option<(u32, Instant)> {
    usage.current().unwrap().rejected.get(&provider).copied()
}

#[test]
fn a_rejected_sign_in_is_asked_less_often_while_others_keep_their_pace() {
    let start = Instant::now();
    let (claude, codex) = (provider("claude"), provider("codex"));
    let local = Host::Local;
    let mut usage = Usage::default();

    read_once(&mut usage, &local, start);
    assert_eq!(again(&usage, claude), Some((1, start + REFRESH * 2)));
    assert_eq!(again(&usage, codex), None);

    // The next refresh leaves Claude out but keeps what it last said, and
    // Codex is read as usual.
    let next = start + REFRESH;
    read_once(&mut usage, &local, next);
    let entry = usage.current().unwrap();
    assert!(entry.held.contains(&claude));
    assert_eq!(entry.readings.len(), 2, "a held provider is not signed out");
    let held = entry
        .readings
        .iter()
        .find(|reading| reading.provider == claude)
        .unwrap();
    assert_eq!(held.error, Some(usage::Error::Rejected.to_string()));
    assert_eq!(again(&usage, claude), Some((1, start + REFRESH * 2)));

    // Once due it is asked again, and a second rejection waits twice as long.
    let due = start + REFRESH * 2;
    read_once(&mut usage, &local, due);
    assert!(!usage.current().unwrap().held.contains(&claude));
    assert_eq!(again(&usage, claude), Some((2, due + REFRESH * 4)));
}

#[test]
fn the_wait_after_rejections_stops_growing_at_its_limit() {
    let mut now = Instant::now();
    let claude = provider("claude");
    let local = Host::Local;
    let mut usage = Usage::default();
    let mut waits = Vec::new();
    for _ in 0..40 {
        read_once(&mut usage, &local, now);
        let (_, at) = again(&usage, claude).unwrap();
        waits.push(at - now);
        now = at;
    }
    assert_eq!(waits[..3], [REFRESH * 2, REFRESH * 4, REFRESH * 8]);
    assert!(waits.windows(2).all(|pair| pair[0] <= pair[1]));
    assert_eq!(waits.last(), Some(&REJECTED_LIMIT));
}

#[test]
fn refresh_allow_and_a_good_answer_ask_a_rejected_sign_in_again() {
    let start = Instant::now();
    let claude = provider("claude");
    let local = Host::Local;

    let mut usage = Usage::default();
    read_once(&mut usage, &local, start);
    usage.refresh(start + REFRESH);
    assert_eq!(again(&usage, claude), None, "Refresh asks at once");

    let mut usage = Usage::default();
    read_once(&mut usage, &local, start);
    usage.allow(claude, start);
    assert_eq!(again(&usage, claude), None, "Allow asks at once");

    let mut usage = Usage::default();
    read_once(&mut usage, &local, start);
    let due = start + REFRESH * 2;
    begin(&mut usage, &local, due);
    usage.apply(
        Message::Reading(local.clone(), claude, Ok(report(claude, 1.))),
        due,
    );
    usage.apply(Message::Done(local.clone(), Ok(())), due);
    assert_eq!(
        again(&usage, claude),
        None,
        "a good answer forgets the rest"
    );
}

#[test]
fn an_outage_between_rejections_keeps_the_wait_growing() {
    let start = Instant::now();
    let claude = provider("claude");
    let local = Host::Local;
    let mut usage = Usage::default();
    read_once(&mut usage, &local, start);

    // Due again, Claude's service is down rather than rejecting the sign-in.
    let due = start + REFRESH * 2;
    begin(&mut usage, &local, due);
    usage.apply(
        Message::Reading(
            local.clone(),
            claude,
            Err(Error::Usage(usage::Error::RateLimited)),
        ),
        due,
    );
    usage.apply(Message::Done(local.clone(), Ok(())), due);
    assert_eq!(again(&usage, claude), Some((1, start + REFRESH * 2)));

    // The sign-in still has not worked, so the next rejection counts second.
    let later = due + REFRESH;
    read_once(&mut usage, &local, later);
    assert_eq!(again(&usage, claude), Some((2, later + REFRESH * 4)));
}

/// The worker's own provider loop leaves a held provider unasked while the
/// others still answer. Both answer from an empty home on a host whose
/// `uname` says Linux and whose `security` finds nothing, so the read never
/// reaches this machine's Keychain or calls a service.
#[cfg(unix)]
#[test]
fn the_worker_does_not_ask_a_held_provider() {
    use crate::usage::{probe::Shell, read_with};
    use std::collections::HashSet;

    let (claude, codex) = (provider("claude"), provider("codex"));
    let config = UsageConfig {
        show_providers: vec![claude.id().into(), codex.id().into()],
        hide_providers: registry::all()
            .filter(|other| ![claude, codex].contains(other))
            .map(|other| other.id().into())
            .collect(),
        ..UsageConfig::default()
    };
    let asked = |held: &HashSet<Provider>| {
        let home = tempfile::tempdir().unwrap();
        let bin = home.path().join("bin");
        std::fs::create_dir(&bin).unwrap();
        for (tool, script) in [("uname", "echo Linux"), ("security", "exit 44")] {
            crate::test_executable::write(bin.join(tool), format!("#!/bin/sh\n{script}\n"), 0o755)
                .unwrap();
        }
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-s")
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut exec = Exec::Remote(Shell::start(command).unwrap());
        let mut asked = Vec::new();
        read_with(
            &mut exec,
            &config,
            &HashSet::new(),
            held,
            &mut CookieJar::default(),
            |provider, _| {
                asked.push(provider);
                true
            },
        );
        asked.into_iter().collect::<HashSet<_>>()
    };
    assert_eq!(asked(&HashSet::new()), HashSet::from([claude, codex]));
    assert_eq!(asked(&HashSet::from([claude])), HashSet::from([codex]));
}

#[test]
fn a_config_change_asks_every_rejected_sign_in_again() {
    let start = Instant::now();
    let claude = provider("claude");
    let local = Host::Local;
    let mut usage = Usage::default();
    let config = UsageConfig::default();
    usage.poll(
        Some(local.clone()),
        &config,
        &Default::default(),
        0,
        false,
        start,
    );
    read_once(&mut usage, &local, start);
    usage.poll(
        Some(local.clone()),
        &config,
        &Default::default(),
        1,
        false,
        start,
    );
    assert_eq!(again(&usage, claude), None);
}
