use super::*;

fn host(id: char, enabled: bool) -> SavedHost {
    SavedHost {
        id: id.to_string().repeat(32),
        label: id.to_string(),
        target: format!("me@{id}"),
        session: "default".to_owned(),
        enabled,
    }
}

#[test]
fn only_an_enabled_device_the_last_read_lacked_is_new() {
    let known = [host('a', true)];
    assert!(!new_device(&known, &known));
    assert!(!new_device(&known, &[]));
    assert!(new_device(&known, &[host('a', true), host('b', true)]));
    // A device turned off in Herdr is not synced, so it starts nothing.
    assert!(!new_device(&known, &[host('a', true), host('b', false)]));
}

#[test]
fn the_suggested_name_is_the_first_label_of_the_host_name() {
    assert_eq!(
        suggestion::short_name("MacBookPro.localdomain"),
        "MacBookPro"
    );
    assert_eq!(suggestion::short_name("MacBookPro"), "MacBookPro");
    assert_eq!(suggestion::short_name(".odd"), ".odd");
    assert_eq!(
        suggestion::address("mac", "MacBookPro.tail1.ts.net"),
        "mac@MacBookPro.tail1.ts.net"
    );
    assert_eq!(suggestion::address("", "studio.local"), "studio.local");
}
