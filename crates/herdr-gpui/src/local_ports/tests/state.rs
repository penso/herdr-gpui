use super::*;
use std::path::Path;

#[test]
fn a_listing_from_the_worker_is_shown_and_a_failure_keeps_it() {
    let (mut ports, updates, _commands) = LocalPorts::fake();
    let now = Instant::now();
    assert!(!ports.poll(true, now));
    assert_eq!(ports.listing(), None);
    updates
        .send(Update::Listing(
            Instant::now(),
            Ok(vec![listener(3000, 4242, 10)]),
        ))
        .unwrap();
    assert!(ports.poll(true, now));
    assert_eq!(ports.listing(), Some(&[listener(3000, 4242, 10)][..]));
    updates
        .send(Update::Listing(Instant::now(), Err(Error::Tool.into())))
        .unwrap();
    assert!(ports.poll(true, now));
    assert!(ports.english_error().unwrap().contains("lsof"));
    assert_eq!(ports.listing().map(<[Listener]>::len), Some(1));
}

#[test]
fn closing_the_section_stops_the_scan_but_keeps_the_listing() {
    let (mut ports, _updates, _commands) = LocalPorts::fake();
    ports.seed(vec![listener(3000, 4242, 10)]);
    assert!(ports.running());
    assert!(!ports.poll(false, Instant::now()));
    assert!(!ports.running());
    assert!(ports.listing().is_some());
}

#[test]
fn a_recent_command_keeps_the_scan_wanted_until_it_expires() {
    let (mut ports, _updates, _commands) = LocalPorts::fake();
    let now = Instant::now();
    ports.demand(now + DEMAND);
    ports.poll(false, now);
    assert!(ports.running());
    ports.poll(false, now + DEMAND + Duration::from_secs(1));
    assert!(!ports.running());
}

#[test]
fn a_signal_is_sent_once_and_its_outcome_reported() {
    let (mut ports, updates, commands) = LocalPorts::fake();
    let victim = Identity {
        pid: 4242,
        started: 10,
    };
    ports.signal(PortAction::Stop, vec![victim]).unwrap();
    assert_eq!(
        commands.try_recv().unwrap(),
        Command::Signal(PortAction::Stop, vec![victim])
    );
    assert!(ports.acting());
    assert!(matches!(
        ports.signal(PortAction::Pause, vec![victim]),
        Err(crate::Error::LocalPorts(Error::Busy))
    ));
    let other = Identity {
        pid: 4343,
        started: 11,
    };
    let third = Identity {
        pid: 4444,
        started: 12,
    };
    updates
        .send(Update::Signalled(
            PortAction::Stop,
            vec![
                (victim, Outcome::Killed),
                (other, Outcome::Exited),
                (third, Outcome::Foreign),
            ],
        ))
        .unwrap();
    assert!(ports.poll(true, Instant::now()));
    assert!(!ports.acting());
    assert_eq!(
        ports.report(),
        Some(Report {
            action: Some(PortAction::Stop),
            done: 1,
            killed: 1,
            gone: 1,
            failed: 1,
        })
    );
}

#[test]
fn a_refresh_asks_the_worker_to_list_now() {
    let (mut ports, _updates, commands) = LocalPorts::fake();
    ports.refresh();
    assert_eq!(commands.try_recv().unwrap(), Command::Refresh);
}

#[test]
fn a_stopped_worker_is_reported() {
    let (mut ports, updates, commands) = LocalPorts::fake();
    drop((updates, commands));
    assert!(ports.poll(true, Instant::now()));
    assert!(!ports.running());
    assert!(ports.error().is_some());
}

#[test]
fn each_process_is_signalled_once_however_many_ports_it_holds() {
    let pending = Pending {
        action: PortAction::Stop,
        listeners: vec![
            listener(3000, 4242, 10),
            listener(3001, 4242, 10),
            listener(8080, 4343, 11),
        ],
    };
    assert_eq!(
        pending.victims(),
        vec![
            Identity {
                pid: 4242,
                started: 10
            },
            Identity {
                pid: 4343,
                started: 11
            }
        ]
    );
}

#[test]
fn a_listener_names_its_address_page_and_uptime() {
    let mut row = listener(3000, 4242, 1_000);
    assert_eq!(row.address(), "localhost:3000");
    assert_eq!(row.url().unwrap().as_str(), "http://localhost:3000/");
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_090);
    assert_eq!(row.uptime(now), Some(Duration::from_secs(90)));
    row.identity.started = 2_000;
    assert_eq!(row.uptime(now), None);
    row.bind = Bind::Any;
    assert_eq!(row.address(), "*:3000");
}

#[test]
fn directories_are_shown_from_the_home_folder() {
    let home = Some(Path::new("/home/dev"));
    assert_eq!(home_relative(Path::new("/home/dev/app"), home), "~/app");
    assert_eq!(home_relative(Path::new("/home/dev"), home), "~");
    assert_eq!(home_relative(Path::new("/srv/app"), home), "/srv/app");
    assert_eq!(
        home_relative(Path::new("/home/developer"), home),
        "/home/developer"
    );
    assert_eq!(
        home_relative(Path::new("/home/dev/app"), None),
        "/home/dev/app"
    );
}

#[test]
fn a_command_never_takes_rows_cached_before_it_for_current() {
    let (mut ports, updates, _commands) = LocalPorts::fake();
    let before = Instant::now();
    updates
        .send(Update::Listing(before, Ok(vec![listener(3000, 4242, 10)])))
        .unwrap();
    ports.poll(true, before);
    // The section shows the cached rows; a later command waits.
    assert!(ports.listing().is_some());
    let requested = before + Duration::from_millis(1);
    assert_eq!(ports.fresh_listing(requested), None);
    // A scan begun before the request, answering after it, is still old.
    updates
        .send(Update::Listing(before, Ok(vec![listener(3000, 4242, 10)])))
        .unwrap();
    ports.poll(true, requested);
    assert_eq!(ports.fresh_listing(requested), None);
    // One begun after it answers.
    let fresh = vec![listener(8080, 4343, 20)];
    updates
        .send(Update::Listing(requested, Ok(fresh.clone())))
        .unwrap();
    ports.poll(true, requested);
    assert_eq!(ports.fresh_listing(requested), Some(Ok(&fresh[..])));
}

#[test]
fn a_fresh_failed_scan_refuses_rather_than_using_old_rows() {
    let (mut ports, updates, _commands) = LocalPorts::fake();
    ports.seed(vec![listener(3000, 4242, 10)]);
    let requested = Instant::now();
    updates
        .send(Update::Listing(requested, Err(Error::Tool.into())))
        .unwrap();
    ports.poll(true, requested);
    let refused = ports.fresh_listing(requested).unwrap().unwrap_err();
    assert!(refused.contains("lsof"), "{refused}");
}

#[test]
fn a_halted_scan_answers_a_command_at_once() {
    let (mut ports, updates, commands) = LocalPorts::fake();
    drop((updates, commands));
    let requested = Instant::now();
    ports.poll(true, requested);
    assert!(ports.fresh_listing(requested).unwrap().is_err());
}
