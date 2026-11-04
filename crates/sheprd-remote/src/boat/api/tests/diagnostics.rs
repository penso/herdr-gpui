use super::*;

#[test]
fn lifecycle_errors_and_unknown_states_are_bounded_single_line_text() {
    let error = format!("failure\n{}", "🦀".repeat(5000));
    let Readiness::Failed(reason) = sandbox("error", Some(&error)).readiness() else {
        panic!("expected a failed sandbox");
    };
    assert_eq!(reason.chars().count(), 300);
    assert!(reason.starts_with("failure "));
    assert!(!reason.chars().any(char::is_control));

    let state = format!("new_state\n{}", "🦀".repeat(5000));
    let Readiness::Pending(reason) = sandbox(&state, None).readiness() else {
        panic!("expected a pending sandbox");
    };
    assert_eq!(reason.chars().count(), 300);
    assert!(reason.starts_with("new state "));
    assert!(!reason.chars().any(char::is_control));
    assert_eq!(
        sandbox("error", Some("\n\0")).readiness(),
        Readiness::Failed("error".into())
    );
    assert_eq!(
        sandbox("\n__\0", None).readiness(),
        Readiness::Pending("unknown".into())
    );
}
