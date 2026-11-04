use super::*;

#[test]
fn commands_keep_their_program() {
    assert_eq!(new_command("git").get_program(), "git");
}
