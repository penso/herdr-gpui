use std::{ffi::OsString, path::PathBuf};

use super::*;

#[test]
fn endpoint_ids_are_32_lowercase_hex_digits() {
    let id = EndpointId::random();
    assert_eq!(id.as_str().parse::<EndpointId>().unwrap(), id);
    let uuid = uuid::Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap();
    assert_eq!(
        EndpointId::from(uuid).as_str(),
        "67e5504410b1426f9247bb680e5fe0c8"
    );
    for bad in [
        "",
        "../../etc/passwd",
        "ABCDEF0123456789abcdef0123456789",
        "abcdef0123456789abcdef012345678",
        "abcdef0123456789abcdef01234567890",
        "abcdef0123456789abcdef012345678g",
    ] {
        assert!(
            matches!(bad.parse::<EndpointId>(), Err(Error::InvalidEndpoint)),
            "{bad}"
        );
    }
}

#[test]
fn endpoint_dir_prefers_xdg_state_home_then_home() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    };
    assert_eq!(
        endpoint_dir_with(env(&[("XDG_STATE_HOME", "/s"), ("HOME", "/h")])),
        Some(PathBuf::from("/s/herdr/acp"))
    );
    assert_eq!(
        endpoint_dir_with(env(&[("XDG_STATE_HOME", ""), ("HOME", "/h")])),
        Some(PathBuf::from("/h/.local/state/herdr/acp"))
    );
    assert_eq!(endpoint_dir_with(env(&[])), None);
}

#[test]
fn endpoint_path_stays_inside_the_directory() {
    let id: EndpointId = "0123456789abcdef0123456789abcdef".parse().unwrap();
    assert_eq!(
        endpoint_path(&PathBuf::from("/d"), &id),
        PathBuf::from("/d/0123456789abcdef0123456789abcdef.sock")
    );
}

#[test]
fn the_address_is_a_socket_on_unix_and_a_pipe_on_windows() {
    let id: EndpointId = "0123456789abcdef0123456789abcdef".parse().unwrap();
    let address = endpoint_address(&id);
    if cfg!(windows) {
        assert_eq!(
            address,
            Some(PathBuf::from(
                r"\\.\pipe\herdr-acp-0123456789abcdef0123456789abcdef"
            ))
        );
    } else {
        assert_eq!(address, endpoint_dir().map(|dir| endpoint_path(&dir, &id)));
    }
}
