#![allow(clippy::unwrap_used)]
use super::*;

fn shell(script: &str) -> Command {
    let mut command = sheprd_client::new_command("/bin/sh");
    command.args(["-c", script]);
    command
}

fn words(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn every_word_is_quoted_and_home_is_expanded() {
    assert_eq!(
        script(&words(&["git", "commit", "-m", "it's done"]), None).unwrap(),
        r#"exec 'git' 'commit' '-m' 'it'\''s done'"#
    );
    assert_eq!(
        script(&words(&["ls"]), Some("~/code/my app")).unwrap(),
        r#"cd "$HOME"'/code/my app' && exec 'ls'"#
    );
    assert_eq!(
        script(&words(&["ls"]), Some("~")).unwrap(),
        r#"cd "$HOME" && exec 'ls'"#
    );
    assert_eq!(
        script(&words(&["ls"]), Some("/srv/$(x)")).unwrap(),
        r#"cd '/srv/$(x)' && exec 'ls'"#
    );
    assert!(matches!(script(&[], None), Err(Error::Empty)));
    assert!(matches!(script(&words(&[""]), None), Err(Error::Empty)));
}

#[test]
fn the_quoted_line_runs_the_words_unchanged() {
    let line = script(&words(&["printf", "%s|", "a b", "$HOME", "'q'"]), None).unwrap();
    let finished = capture(
        shell(&line),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(finished.stdout, "a b|$HOME|'q'|");
}

#[test]
fn streams_are_kept_apart_with_the_exit_code_and_without_controls() {
    let finished = capture(
        shell("printf 'out\\033[31mred\\033[0m\\n'; printf 'err\\a\\n' >&2; exit 3"),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(finished.code, Some(3));
    assert_eq!(finished.stdout, "outred\n");
    assert_eq!(finished.stderr, "err\n");
    assert!(!finished.truncated);
}

#[test]
fn only_the_tail_of_long_output_is_kept() {
    let finished = capture(
        shell("head -c 200000 /dev/zero | tr '\\0' a; echo END"),
        Duration::from_secs(10),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(finished.truncated);
    assert_eq!(finished.stdout.len(), STDOUT_LIMIT);
    assert!(finished.stdout.ends_with("aaaEND\n"));
}

#[test]
fn invalid_utf8_stays_inside_the_limit_once_decoded() {
    let tail = Tail::new(STDOUT_LIMIT);
    tail.kept.lock().unwrap().0 = [vec![0xff; STDOUT_LIMIT - 3], b"END".to_vec()].concat();
    let (text, truncated) = tail.text();
    assert!(truncated);
    assert!(text.len() <= STDOUT_LIMIT);
    assert!(text.ends_with("\u{fffd}END"));
}

#[test]
fn a_command_past_its_time_or_cancelled_is_stopped() {
    let started = Instant::now();
    let timed_out = capture(
        shell("sleep 30"),
        Duration::from_millis(200),
        &AtomicBool::new(false),
    );
    assert!(matches!(timed_out, Err(Error::Timeout(0))));
    let cancelled = capture(
        shell("sleep 30"),
        Duration::from_secs(30),
        &AtomicBool::new(true),
    );
    assert!(matches!(cancelled, Err(Error::Cancelled)));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn time_spent_reaching_the_machine_comes_out_of_the_command_s() {
    let timeout = Duration::from_secs(120);
    let spent = until(Instant::now() - Duration::from_secs(1), timeout, |_| {
        panic!("a command past its deadline must not start")
    });
    assert!(matches!(spent, Err(Error::Timeout(120))));
    let mut given = Duration::ZERO;
    let started = until(Instant::now() + Duration::from_secs(30), timeout, |left| {
        given = left;
        Err(Error::Timeout(left.as_secs()))
    });
    assert!(matches!(started, Err(Error::Timeout(120))));
    assert!(given <= Duration::from_secs(30) && given > Duration::from_secs(25));
}
