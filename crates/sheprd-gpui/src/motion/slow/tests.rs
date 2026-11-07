use super::*;

#[test]
fn slow_time_passes_a_tenth_as_fast_from_when_it_was_turned_on() {
    let on = Instant::now();
    let later = on + Duration::from_secs(10);
    assert_eq!(at(later, None), later);
    assert_eq!(at(later, Some(on)), on + Duration::from_secs(1));
    assert_eq!(at(on, Some(on)), on, "no jump when it turns on");
    // Time read before the switch stays where it was.
    let earlier = on.checked_sub(Duration::from_secs(1)).unwrap_or(on);
    assert_eq!(at(earlier, Some(on)), on);
}

#[test]
fn slow_animations_last_ten_times_longer() {
    let length = Duration::from_millis(180);
    assert_eq!(stretch(length, false), length);
    assert_eq!(stretch(length, true), Duration::from_millis(1800));
}
