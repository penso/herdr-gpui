use super::*;

fn read(source: &Source, address: &str) -> Result<Value> {
    source.call(ApiMethod::PaneRead, &json!({"pane_id": id(address)}))
}

fn numbered(count: usize) -> (Vec<String>, Vec<Value>) {
    let addresses = (1..=count).map(|n| format!("t{n}")).collect::<Vec<_>>();
    let records = addresses.iter().map(|address| record(address)).collect();
    (addresses, records)
}

#[test]
fn watching_observes_and_typing_takes_the_driver_role_once() {
    let (source, fake) = fake();
    source.poll().unwrap();
    read(&source, "t1").unwrap();
    let watching = fake.attaches.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(watching["observe"], true);
    let text = json!({"pane_id": id("t1"), "text": "a"});
    source.call(ApiMethod::PaneSendText, &text).unwrap();
    let driving = fake.attaches.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(driving.get("observe").is_none());
    assert_eq!(
        (&driving["cols"], &driving["rows"]),
        (&json!(80), &json!(24))
    );
    source.call(ApiMethod::PaneSendText, &text).unwrap();
    assert!(fake.attaches.try_recv().is_err());
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_new_pane_takes_the_place_of_the_screen_idle_longest() {
    let (addresses, records) = numbered(MAX_SCREENS + 1);
    let (source, fake) = fake_with_doc(false, Value::Null, records);
    source.poll().unwrap();
    for address in &addresses[..MAX_SCREENS] {
        read(&source, address).unwrap();
    }
    {
        let state = lock(&source.state);
        for (address, age) in [("t3", 10), ("t4", 20)] {
            lock(&state.screens[address]).age(Duration::from_secs(age));
        }
    }
    read(&source, &addresses[MAX_SCREENS]).unwrap();
    let state = lock(&source.state);
    assert_eq!(state.screens.len(), MAX_SCREENS);
    assert!(!state.screens.contains_key("t4"));
    assert!(state.screens.contains_key("t3"));
    drop(state);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn every_screen_in_use_keeps_a_new_pane_out() {
    let (addresses, records) = numbered(MAX_SCREENS + 1);
    let (source, fake) = fake_with_doc(false, Value::Null, records);
    source.poll().unwrap();
    for address in &addresses[..MAX_SCREENS] {
        read(&source, address).unwrap();
    }
    assert!(matches!(
        read(&source, &addresses[MAX_SCREENS]).unwrap_err(),
        HubError::Busy
    ));
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_held_pane_does_not_stop_another() {
    let (_, records) = numbered(2);
    let (source, fake) = fake_with_doc(false, Value::Null, records);
    source.poll().unwrap();
    read(&source, "t1").unwrap();
    read(&source, "t2").unwrap();
    let held = lock(&source.state).screens["t1"].clone();
    let guard = lock(&held);
    read(&source, "t2").unwrap();
    source.poll().unwrap();
    assert!(lock(&source.state).screens.contains_key("t1"));
    drop(guard);
    drop(source);
    fake.server.join().unwrap();
}

#[test]
fn a_terminal_event_ends_the_wait_for_the_next_poll() {
    let (source, fake) = fake();
    source.poll().unwrap();
    let core = lock(&source.state).core.clone().unwrap();
    let mut events = Some(core.subscribe().unwrap());
    let started = Instant::now();
    wait(&mut events, Duration::from_secs(30));
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(events.is_some());
    drop((events, core, source));
    fake.server.join().unwrap();
}

#[test]
fn without_events_the_wait_ends_at_once() {
    let started = Instant::now();
    wait(&mut None, Duration::from_secs(30));
    assert!(started.elapsed() < Duration::from_secs(10));
}
