use super::*;

#[test]
fn a_send_recovers_early_and_late_fallbacks_once_each() {
    let group = Copies::default();
    group.extend(["shell note".into()]);
    assert_eq!(group.text().as_deref(), Some("shell note"));
    let mut first_tick = Vec::new();
    collect_copy(
        &mut first_tick,
        Some(group.clone()),
        Some("first agent".into()),
    );
    collect_copy(
        &mut first_tick,
        Some(group.clone()),
        Some("second agent".into()),
    );
    assert_eq!(first_tick.len(), 1, "one clipboard prompt per send group");
    assert_eq!(
        first_tick[0].text().as_deref(),
        Some("shell note\nfirst agent\nsecond agent")
    );
    let mut later_tick = Vec::new();
    collect_copy(&mut later_tick, Some(group), Some("late agent".into()));
    assert_eq!(
        later_tick[0].text().as_deref(),
        Some("shell note\nfirst agent\nsecond agent\nlate agent")
    );
    collect_copy(&mut later_tick, None, None);
    assert_eq!(later_tick.len(), 1);
}
