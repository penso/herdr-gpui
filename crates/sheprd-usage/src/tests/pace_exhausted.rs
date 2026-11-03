use super::*;

#[test]
fn a_used_up_quota_says_it_ran_out() {
    let now = at(1_000_000);
    let window = |used| Window::new(Kind::Weekly, used, Some(now + WEEK / 2), Some(WEEK));
    for used in [100_f32, 130.] {
        let pace = window(used.into()).pace(now).unwrap();
        assert_eq!(pace.describe(used), "Ran out");
    }
    let pace = window(99.).pace(now).unwrap();
    assert!(pace.describe(99.).contains("in deficit"));
}
