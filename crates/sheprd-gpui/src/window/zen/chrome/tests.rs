use super::*;

/// One frame at 60 Hz.
const FRAME: Duration = Duration::from_micros(16_667);

/// Draws frames from `start` until `elapsed` of slide has run, as the
/// window does, and returns the time of the last one.
fn run(chrome: &mut Chrome, start: Instant, elapsed: Duration) -> Instant {
    let mut now = start;
    chrome.tick(now);
    let frames = (elapsed.as_secs_f64() / FRAME.as_secs_f64()).round() as u32;
    for _ in 0..frames {
        now += FRAME;
        chrome.tick(now);
    }
    now
}

/// Entering, the status bar slides off alone, and the sidebar starts only
/// once the bar has gone.
#[test]
fn hiding_takes_the_status_bar_then_the_sidebar() {
    let start = Instant::now();
    let mut chrome = Chrome::default();
    chrome.hide(true);
    chrome.tick(start);
    assert_eq!(chrome.at(), (Shown::ALL, true));

    let now = run(&mut chrome, start, HIDE_BAR / 2);
    let (mid_bar, moving) = chrome.at();
    assert!(moving);
    assert!(mid_bar.bar > 0.3 && mid_bar.bar < 0.7, "{mid_bar:?}");
    assert_eq!(mid_bar.sidebar, 1.);

    let now = run(&mut chrome, now, HIDE_BAR / 2 + FRAME);
    let (bar_gone, _) = chrome.at();
    assert_eq!(bar_gone.bar, 0.);
    assert!(bar_gone.sidebar > 0.99, "{bar_gone:?}");

    let now = run(&mut chrome, now, HIDE_SIDEBAR / 2);
    let (mid_sidebar, moving) = chrome.at();
    assert!(moving);
    assert_eq!(mid_sidebar.bar, 0.);
    assert!(mid_sidebar.sidebar > 0. && mid_sidebar.sidebar < 1.);

    run(&mut chrome, now, HIDE_SIDEBAR / 2 + FRAME);
    assert_eq!(chrome.at(), (Shown::NONE, false));
}

/// The frame that lays the window out for zen mode may be slow. It still
/// shows the slide's start, and a slow frame later on moves the slide only
/// as far as a quick one would have, so no part of it is skipped.
#[test]
fn a_slow_frame_never_skips_the_slide() {
    let start = Instant::now();
    let mut chrome = Chrome::default();
    chrome.hide(true);
    // The first frame drawn comes a long while after the toggle.
    chrome.tick(start + Duration::from_millis(400));
    assert_eq!(chrome.at(), (Shown::ALL, true));
    // And the next one a long while after that.
    chrome.tick(start + Duration::from_millis(800));
    let (shown, moving) = chrome.at();
    assert!(moving);
    assert!(shown.bar > 0.9, "{shown:?}");
    assert_eq!(shown.sidebar, 1.);
}

/// Each step starts gently rather than jumping: a tenth of the way in, a
/// part has moved far less than a tenth of its way.
#[test]
fn each_hiding_step_starts_gently() {
    let start = Instant::now();
    let mut chrome = Chrome::default();
    chrome.hide(true);
    let now = run(&mut chrome, start, HIDE_BAR / 10);
    assert!(chrome.at().0.bar > 0.98, "{:?}", chrome.at());
    run(
        &mut chrome,
        now,
        HIDE_BAR - HIDE_BAR / 10 + HIDE_SIDEBAR / 10,
    );
    assert!(chrome.at().0.sidebar > 0.98, "{:?}", chrome.at());
}

/// Entering is slower than leaving, which brings both back together.
#[test]
fn showing_brings_both_back_together_and_quicker() {
    let start = Instant::now();
    let mut chrome = Chrome::Still(Shown::NONE);
    chrome.show(true);
    let now = run(&mut chrome, start, SHOW / 2);
    let (mid, moving) = chrome.at();
    assert!(moving);
    assert_eq!(mid.bar, mid.sidebar);
    assert!(mid.bar > 0.5, "eases out: {mid:?}");
    run(&mut chrome, now, SHOW / 2 + FRAME);
    assert_eq!(chrome.at(), (Shown::ALL, false));
    assert!(SHOW < HIDE_BAR + HIDE_SIDEBAR);
}

/// Toggling back mid-way continues from where each part is, never from an
/// end.
#[test]
fn reversing_mid_way_starts_from_where_it_is() {
    let start = Instant::now();
    let mut chrome = Chrome::default();
    chrome.hide(true);
    let now = run(&mut chrome, start, HIDE_BAR + HIDE_SIDEBAR / 2);
    let (before, _) = chrome.at();
    chrome.show(true);
    assert_eq!(chrome.at().0, before);
    let now = run(&mut chrome, now, SHOW / 4);
    let (from, _) = chrome.at();
    assert!(from.sidebar > before.sidebar);

    // Hiding again from part-way, each part travels only what it has left,
    // so the sidebar follows without the bar's full wait.
    chrome.hide(true);
    assert_eq!(chrome.at().0, from);
    let left = HIDE_BAR.mul_f32(from.bar) + HIDE_SIDEBAR.mul_f32(from.sidebar);
    let now = run(&mut chrome, now, left - FRAME * 2);
    assert!(chrome.at().1);
    run(&mut chrome, now, FRAME * 3);
    assert_eq!(chrome.at(), (Shown::NONE, false));
}

#[test]
fn without_animation_it_goes_and_comes_at_once() {
    let mut chrome = Chrome::default();
    chrome.hide(false);
    assert_eq!(chrome.at(), (Shown::NONE, false));
    chrome.show(false);
    assert_eq!(chrome.at(), (Shown::ALL, false));
}
