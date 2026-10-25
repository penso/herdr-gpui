use super::*;
use crate::control::{Request, Response};

#[tokio::test]
async fn herdr_ui_sets_the_marks_and_they_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let state = state(dir.path());
    assert_eq!(state.look.indicators(), Indicators::Dots);
    let (stop, _) = tokio::sync::watch::channel(false);
    let answer = crate::server::answer_for_tests(
        &state,
        &stop,
        Request::Indicators {
            set: "tiles".into(),
        },
    );
    assert_eq!(answer, Response::Done);
    assert_eq!(state.look.indicators(), Indicators::Tiles);
    assert_eq!(Look::load(dir.path()).indicators(), Indicators::Tiles);
}
