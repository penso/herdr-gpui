use super::*;

#[test]
fn non_local_paths_are_classified_before_peer_credentials() -> anyhow::Result<()> {
    use nix::unistd::Uid;
    // macOS's default temporary directory can exceed sockaddr_un's path bound.
    let root = tempfile::tempdir_in("/tmp")?;
    let local = root.path().join("herdr-client.sock");
    let forwarded = root.path().join("forwarded.sock");
    let _local_listener = UnixListener::bind(&local)?;
    let _forwarded_listener = UnixListener::bind(&forwarded)?;

    for credentials in [
        Ok(Uid::from_raw(nix::unistd::geteuid().as_raw() ^ 1)),
        Err(nix::errno::Errno::EPERM),
    ] {
        assert_eq!(
            check_local_endpoint(&forwarded, &local, || credentials),
            Err(UntrustedEndpoint::OtherSocket)
        );
        assert_eq!(
            check_local_endpoint(&local, &local, || credentials),
            Err(UntrustedEndpoint::PeerUser)
        );
    }
    assert_eq!(
        check_local_endpoint(&forwarded, &root.path().join("missing.sock"), || {
            panic!("non-local endpoints must not inspect peer credentials")
        }),
        Err(UntrustedEndpoint::NotSocket)
    );
    Ok(())
}
