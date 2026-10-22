use super::*;

#[test]
fn macos_does_not_add_linuxbrew_to_command_paths() {
    let linuxbrew = Path::new("/home/linuxbrew/.linuxbrew/bin");
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    assert_eq!(
        fallback().iter().filter(|dir| *dir == linuxbrew).count(),
        std::env::split_paths(&inherited)
            .filter(|dir| dir == linuxbrew)
            .count(),
        "only a Linuxbrew path explicitly inherited from the user may remain"
    );
}
