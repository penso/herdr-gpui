use super::*;

#[test]
fn the_daemon_keeps_its_state_beside_the_app_s() {
    if let (Ok(dir), Some(app)) = (state_dir(), crate::preferences::state_dir()) {
        assert_eq!(dir, app.join("remote"));
    }
}
