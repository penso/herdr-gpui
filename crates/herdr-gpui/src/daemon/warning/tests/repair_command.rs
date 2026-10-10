use super::*;

#[test]
fn displayed_repair_command_targets_the_exact_path() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for reason in [
        UntrustedEndpoint::DirectoryPermissions,
        UntrustedEndpoint::SocketPermissions,
    ] {
        // Keep the real socket below macOS's sockaddr_un path bound.
        let root = tempfile::tempdir_in("/tmp")?;
        let directory = root.path().join("it's a local session");
        std::fs::create_dir(&directory)?;
        let socket = directory.join("herdr-client.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket)?;
        let path = match reason {
            UntrustedEndpoint::DirectoryPermissions => &directory,
            _ => &socket,
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o777))?;
        let notice =
            LocalPeerWarning::new(reason, socket.clone()).notice(std::time::Instant::now());
        let body = notice
            .body
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing recovery body"))?;
        let command = body
            .lines()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("missing standalone command"))?;
        assert!(command.starts_with("chmod "));
        assert_eq!(body.lines().count(), 2);
        assert!(
            std::process::Command::new("sh")
                .args(["-c", command])
                .status()?
                .success()
        );
        let expected = match reason {
            UntrustedEndpoint::DirectoryPermissions => 0o775,
            _ => 0o755,
        };
        assert_eq!(
            std::fs::metadata(path)?.permissions().mode() & 0o777,
            expected
        );
    }
    Ok(())
}
