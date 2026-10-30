use super::*;
use std::time::Instant;

#[test]
fn runs_are_spaced_by_the_gap() {
    let start = Instant::now();
    let mut pace = Pace::new(Duration::from_millis(100));
    assert!(pace.take(start));
    assert!(!pace.take(start + Duration::from_millis(40)));
    assert_eq!(
        pace.wait(start + Duration::from_millis(40)),
        Duration::from_millis(60)
    );
    assert!(pace.take(start + Duration::from_millis(100)));
}
