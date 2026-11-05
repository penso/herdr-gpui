use super::*;

#[test]
fn a_running_machine_is_asked_to_save_once() {
    assert_eq!(step(&Status::Running, false), Step::Suspend);
    // Its save failed and left it running: it must not hold the quit forever.
    assert_eq!(step(&Status::Running, true), Step::Done);
}

#[test]
fn a_machine_on_its_way_up_is_waited_for_then_saved() {
    assert_eq!(step(&Status::Starting, false), Step::Wait);
    assert_eq!(step(&Status::Running, false), Step::Suspend);
}

#[test]
fn a_saving_machine_holds_the_quit() {
    assert_eq!(step(&Status::Saving, true), Step::Wait);
}

#[test]
fn a_stopped_or_failed_machine_lets_the_quit_go() {
    assert_eq!(step(&Status::Stopped, true), Step::Done);
    assert_eq!(step(&Status::Failed("no".to_owned()), false), Step::Done);
}
