#![allow(clippy::unwrap_used)]
use super::*;

fn parse(text: &str) -> PhoneConfig {
    toml::from_str(text).unwrap()
}

fn json(request: &Request) -> serde_json::Value {
    serde_json::from_slice(&request.body).unwrap()
}

#[test]
fn service_resolution_prefers_explicit_choice_and_reports_gaps() {
    assert!(matches!(
        parse("").service(),
        Err(Error::PhoneNotConfigured)
    ));
    assert!(matches!(
        parse("[ntfy]\ntopic = '  '").service(),
        Err(Error::PhoneNotConfigured)
    ));
    assert!(matches!(
        parse("[pushover]\ntoken = 'a'").service(),
        Err(Error::PhoneNotConfigured)
    ));
    let both = "[ntfy]\ntopic = 't'\n[pushover]\ntoken = 'a'\nuser = 'u'\n";
    assert!(matches!(
        parse(both).service(),
        Err(Error::PhoneServiceAmbiguous)
    ));
    assert!(matches!(
        parse(&format!("service = 'pushover'\n{both}")).service(),
        Ok(Service::Pushover { .. })
    ));
    assert!(matches!(
        parse("service = 'pushover'\n[ntfy]\ntopic = 't'").service(),
        Err(Error::PhoneNotConfigured)
    ));
    let Ok(Service::Ntfy { server, token, .. }) = parse("[ntfy]\ntopic = 't'").service() else {
        panic!("expected ntfy");
    };
    assert_eq!(server.as_str(), DEFAULT_NTFY);
    assert!(token.is_none());
    let Ok(Service::Ntfy { server, .. }) =
        parse("[ntfy]\ntopic = 't'\nserver = 'http://box.lan:8080/ntfy'").service()
    else {
        panic!("expected ntfy");
    };
    assert_eq!(server.as_str(), "http://box.lan:8080/ntfy/");
}

#[test]
fn events_need_opt_in_a_service_and_only_cover_blocked_and_done() {
    let mut config = parse("blocked = true\ndone = false\n[ntfy]\ntopic = 't'");
    assert_eq!(config.events(), PhoneEvents::default());
    config.enabled = true;
    let events = config.events();
    assert!(events.wants(Kind::NeedsAttention));
    assert!(!events.wants(Kind::Finished));
    let all = PhoneEvents {
        blocked: true,
        done: true,
    };
    assert!(!all.wants(Kind::Custom) && !all.wants(Kind::UpdateInstalled));
    config.ntfy.topic = None;
    assert_eq!(config.events(), PhoneEvents::default());
    assert!(parse("").blocked && parse("").done && !parse("").enabled);
}

#[test]
fn validation_rejects_bad_servers_and_topics_without_echoing_them() {
    for server in [
        "ftp://ntfy.sh",
        "https://user:pw@ntfy.sh",
        "https://ntfy.sh/?x=1",
        "https://ntfy.sh/#a",
        "not a url",
    ] {
        let config = parse(&format!("[ntfy]\nserver = '{server}'"));
        assert!(
            matches!(config.validate(), Err(Error::PhoneServer)),
            "{server}"
        );
    }
    for topic in ["has space", "slash/topic", &"a".repeat(65)] {
        let error = parse(&format!("[ntfy]\ntopic = '{topic}'"))
            .validate()
            .unwrap_err();
        assert!(matches!(error, Error::PhoneTopic));
        assert!(!error.to_string().contains(topic));
    }
    parse("[ntfy]\ntopic = 'herdr-Box_1'\nserver = 'https://push.example.com'")
        .validate()
        .unwrap();
    assert!(toml::from_str::<PhoneConfig>("[ntfy]\npassword = 'x'").is_err());
    assert!(toml::from_str::<PhoneConfig>("service = 'email'").is_err());
}

#[test]
fn messages_are_bounded_control_free_and_never_empty() {
    let message = Message::new(
        Event::Blocked,
        &"\u{1b}[31m界".repeat(100),
        Some(&"line\n".repeat(100)),
        Some("build box"),
    );
    assert!(message.title.chars().count() <= TITLE_LIMIT);
    assert!(message.title.contains('界'));
    assert!(message.title.chars().all(|c| !c.is_control()));
    assert!(message.body.starts_with("build box: line"));
    assert!(message.body.chars().count() <= BODY_LIMIT);
    assert!(!message.body.contains('\n'));
    assert_eq!(
        Message::new(Event::Done, "Done", Some("\n\u{202e}"), None).body,
        "An agent finished."
    );
}

#[test]
fn ntfy_publishes_json_to_the_root_with_bearer_token() {
    let config =
        parse("[ntfy]\ntopic = 'secret-topic'\ntoken = 'tk_1'\nserver = 'https://n.example/'");
    let message = Message::new(Event::Blocked, "Claude", Some("Needs input"), None);
    let request = request(&config.service().unwrap(), &message).unwrap();
    assert_eq!(request.url, "https://n.example/");
    assert!(!request.url.contains("secret-topic"));
    assert_eq!(
        request.authorization.as_deref().map(String::as_str),
        Some("Bearer tk_1")
    );
    assert_eq!(
        json(&request),
        serde_json::json!({
            "topic": "secret-topic",
            "title": "Claude",
            "message": "Needs input",
            "priority": 4,
            "tags": ["warning"],
        })
    );
    let done = Message::new(Event::Done, "Claude", None, None);
    assert_eq!(
        json(&super::request(&config.service().unwrap(), &done).unwrap())["priority"],
        3
    );
}

#[test]
fn pushover_sends_keys_in_the_body_only() {
    let config = parse("[pushover]\ntoken = 'app'\nuser = 'usr'");
    let request = request(&config.service().unwrap(), &Message::test()).unwrap();
    assert_eq!(request.url, PUSHOVER_URL);
    assert!(request.authorization.is_none());
    assert_eq!(
        json(&request),
        serde_json::json!({
            "token": "app",
            "user": "usr",
            "title": "Herdr test",
            "message": "Phone notifications are working.",
        })
    );
}

#[test]
fn debug_output_never_shows_secrets() {
    let config = parse("[ntfy]\ntopic = 'hidden-topic'\ntoken = 'hidden-token'");
    let text = format!("{config:?} {:?}", config.service().unwrap());
    assert!(!text.contains("hidden-topic") && !text.contains("hidden-token"));
}

#[test]
fn status_codes_map_to_typed_errors() {
    assert!(status(200).is_ok() && status(204).is_ok());
    assert!(matches!(status(401), Err(Error::PhoneRejected(401))));
    assert!(matches!(status(403), Err(Error::PhoneRejected(403))));
    assert!(matches!(status(429), Err(Error::PhoneRateLimited)));
    assert!(matches!(status(500), Err(Error::PhoneStatus(500))));
    assert!(matches!(status(302), Err(Error::PhoneStatus(302))));
}

#[test]
fn limiter_allows_a_burst_then_slides() {
    let now = Instant::now();
    let mut limiter = Limiter::default();
    for _ in 0..RATE_LIMIT {
        assert!(limiter.allow(now));
    }
    assert!(!limiter.allow(now + RATE_WINDOW - Duration::from_nanos(1)));
    // The whole burst expires together, so a fresh budget starts.
    for _ in 0..RATE_LIMIT {
        assert!(limiter.allow(now + RATE_WINDOW));
    }
    assert!(!limiter.allow(now + RATE_WINDOW));
}

#[test]
fn dispatcher_drops_cross_window_duplicates_but_not_later_events() {
    let now = Instant::now();
    let mut dispatcher = Dispatcher::default();
    assert!(dispatcher.admit("herdr:local:boot:p1", now));
    assert!(!dispatcher.admit("herdr:local:boot:p1", now + Duration::from_secs(1)));
    assert!(dispatcher.admit("herdr:local:boot:p2", now + Duration::from_secs(1)));
    assert!(dispatcher.admit("herdr:local:boot:p1", now + DUPLICATE_WINDOW));
    // Rejected duplicates do not spend the rate budget.
    assert_eq!(dispatcher.limiter.sent.len(), 3);
}

#[test]
fn backoff_after_429_skips_until_it_expires() {
    let now = Instant::now();
    let mut backoff = Backoff::default();
    assert!(matches!(
        backoff.run(now, || Err(Error::PhoneRateLimited)),
        Some(Err(Error::PhoneRateLimited))
    ));
    let mut called = false;
    assert!(
        backoff
            .run(now + BACKOFF - Duration::from_nanos(1), || {
                called = true;
                Ok(())
            })
            .is_none()
    );
    assert!(!called);
    assert!(matches!(
        backoff.run(now + BACKOFF, || Ok(())),
        Some(Ok(()))
    ));
    // Other failures do not pause delivery.
    assert!(
        backoff
            .run(now + BACKOFF, || Err(Error::PhoneStatus(500)))
            .is_some()
    );
    assert!(backoff.run(now + BACKOFF, || Ok(())).is_some());
}
