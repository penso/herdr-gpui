use super::*;

#[test]
fn a_log_keeps_its_end_whatever_its_size() {
    let log = vec![b'a'; 100_000]
        .into_iter()
        .chain(*b"the end")
        .collect::<Vec<_>>();
    assert_eq!(tail(log.as_slice(), 7, u64::MAX).unwrap(), b"the end");
}

#[test]
fn a_log_is_read_no_further_than_the_limit() {
    let log = b"0123456789";
    assert_eq!(tail(log.as_slice(), 3, 5).unwrap(), b"234");
}

#[test]
fn the_token_free_redirect_must_be_encrypted() {
    let url = "https://storage.example/log?sig=1";
    assert_eq!(storage(302, Some(url)), Some(url));
    assert_eq!(storage(302, Some("http://storage.example/log")), None);
    assert_eq!(storage(200, Some(url)), None);
    assert_eq!(storage(302, None), None);
}

#[test]
fn refusals_keep_their_meaning() {
    assert!(matches!(refusal(401), crate::Error::Authentication));
    assert!(matches!(refusal(403), crate::Error::Forbidden));
    assert!(matches!(refusal(404), crate::Error::Status(404)));
    assert!(matches!(
        refusal(302),
        crate::Error::CheckLog(Error::Location)
    ));
}

#[test]
fn an_excerpt_ends_on_the_last_error_without_timestamps_or_colors() {
    let log = "\u{feff}2026-01-02T03:04:05.1234567Z first\n\
        2026-01-02T03:04:06.1234567Z \u{1b}[31mtest failed\u{1b}[0m\n\
        2026-01-02T03:04:07.1234567Z ##[error]Process completed with exit code 1.\n\
        2026-01-02T03:04:08.1234567Z Post job cleanup.\n";
    assert_eq!(
        excerpt(log),
        "first\ntest failed\n##[error]Process completed with exit code 1."
    );
}

#[test]
fn a_log_without_a_marked_error_gives_its_end() {
    let log = (0..500)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let text = excerpt(&log);
    assert!(text.ends_with("line 499"));
    assert!(!text.contains("line 100\n"));
}

#[test]
fn an_excerpt_stays_within_its_limit_and_starts_on_a_whole_line() {
    let log = (0..70)
        .map(|line| format!("{line} {}", "é".repeat(400)))
        .collect::<Vec<_>>()
        .join("\n");
    let text = excerpt(&log);
    assert!(text.len() <= MAX_EXCERPT);
    assert!(text.starts_with(|c: char| c.is_ascii_digit()));
    assert!(text.ends_with('é'));
}

#[test]
fn control_and_direction_characters_do_not_reach_a_prompt() {
    assert_eq!(plain("a\u{7}b\u{202e}c\td"), "abc    d");
}

#[test]
fn an_empty_log_gives_an_empty_excerpt() {
    assert_eq!(excerpt(""), "");
}
