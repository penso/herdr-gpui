use super::*;

struct Buffered {
    deadline: Option<Instant>,
    delay: Duration,
    pending: Option<Value>,
}

impl Buffered {
    fn new(delay: Duration) -> Self {
        Self {
            deadline: None,
            delay,
            pending: None,
        }
    }
}

impl Translator for Buffered {
    fn start(&mut self) -> Vec<Out> {
        Vec::new()
    }

    fn on_session(&mut self, _: Inbound) -> Vec<Out> {
        Vec::new()
    }

    fn on_harness(&mut self, message: &Value) -> Vec<Out> {
        self.pending = Some(message.clone());
        self.deadline
            .get_or_insert_with(|| Instant::now() + self.delay);
        Vec::new()
    }

    fn next_tick(&self) -> Option<Instant> {
        self.deadline
    }

    fn tick(&mut self) -> Vec<Out> {
        self.deadline = None;
        self.pending
            .take()
            .map(|message| out::reply(wire::RequestId::Number(7), Ok(message)))
            .into_iter()
            .collect()
    }
}

fn assert_partial(inbound: Inbound) {
    let Inbound::Response { result, .. } = inbound else {
        panic!("{inbound:?}");
    };
    assert_eq!(result.unwrap(), json!({"partial": "available"}));
}

#[test]
fn a_pending_update_flushes_while_the_harness_is_idle() {
    let mut wired = wire(Box::new(Translated(Buffered::new(Duration::from_millis(
        10,
    )))));
    wired.write(r#"{"partial":"available"}"#);
    assert_partial(wired.next());
    drop(wired.harness_output);
    assert!(matches!(
        wired.delivered.recv_timeout(WAIT).unwrap(),
        Inbound::Closed(None)
    ));
}

#[test]
fn eof_flushes_pending_output_before_close_even_before_its_deadline() {
    let mut wired = wire(Box::new(Translated(Buffered::new(Duration::from_secs(60)))));
    wired.write(r#"{"partial":"available"}"#);
    drop(wired.harness_output);
    assert_partial(wired.delivered.recv_timeout(WAIT).unwrap());
    assert!(matches!(
        wired.delivered.recv_timeout(WAIT).unwrap(),
        Inbound::Closed(None)
    ));
}

#[test]
fn idle_and_flushed_translators_have_no_deadline() {
    assert!(Relay::default().next_tick().is_none());
    let mut buffered = Buffered::new(Duration::from_millis(50));
    assert!(buffered.next_tick().is_none());
    buffered.on_harness(&json!({"partial": "available"}));
    assert!(buffered.next_tick().is_some());
    assert_eq!(buffered.tick().len(), 1);
    assert!(buffered.next_tick().is_none());
    assert!(buffered.tick().is_empty());
}
