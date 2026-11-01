use super::*;
use keys::{PublicKey, missing_lines};

#[test]
fn only_ed25519_keys_of_base64_are_accepted() {
    assert!(PublicKey::parse("ssh-ed25519 AAAA+/= c").is_ok());
    assert!(matches!(
        PublicKey::parse("ssh-rsa AAAA"),
        Err(Error::PublicKey)
    ));
    assert!(matches!(
        PublicKey::parse("ssh-ed25519 AA'$(x)"),
        Err(Error::PublicKey)
    ));
    assert!(matches!(PublicKey::parse(""), Err(Error::PublicKey)));
}

#[test]
fn the_comment_names_the_machine_in_shell_safe_characters() {
    assert_eq!(
        key(1).line("my mac's 'pro'"),
        "ssh-ed25519 AAAAC3Nza01 herdr-sync:mymacspro"
    );
    assert_eq!(
        key(1).line("  "),
        "ssh-ed25519 AAAAC3Nza01 herdr-sync:machine"
    );
}

#[test]
fn a_key_present_under_any_comment_is_not_added_again() {
    let existing = "ssh-ed25519 AAAAC3Nza01 someone@else\n";
    let (append, added) = missing_lines(
        existing,
        &[
            (key(1), "a".to_owned()),
            (key(2), "b".to_owned()),
            (key(2), "b".to_owned()),
        ],
    );
    assert_eq!(added, 1);
    assert_eq!(append, "ssh-ed25519 AAAAC3Nza02 herdr-sync:b\n");
}

#[test]
fn a_last_line_without_its_end_is_ended_first() {
    let (append, added) = missing_lines("ssh-rsa AAAA x", &[(key(1), "a".to_owned())]);
    assert_eq!(added, 1);
    assert_eq!(append, "\nssh-ed25519 AAAAC3Nza01 herdr-sync:a\n");
    assert_eq!(missing_lines("", &[]), (String::new(), 0));
}
