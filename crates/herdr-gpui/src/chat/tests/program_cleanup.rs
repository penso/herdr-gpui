use super::*;

#[test]
fn fixture_drop_closes_its_listener_and_explicit_stop_is_idempotent() {
    for explicitly_stopped in [false, true] {
        let mut program = Program::new();
        let path = program.path.clone();
        if explicitly_stopped {
            program.stop();
            program.stop();
        }
        drop(program);
        assert!(Connection::connect(&path, Duration::from_millis(100)).is_err());
    }
}
