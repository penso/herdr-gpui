use super::*;
use interprocess::os::windows::{
    ToWtf16 as _,
    named_pipe::{PipeListener, PipeListenerOptions, pipe_mode},
    security_descriptor::SecurityDescriptor,
};
use std::{
    fs::OpenOptions,
    io,
    os::windows::io::AsHandle as _,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

fn listener(
    sddl: &str,
) -> (
    PathBuf,
    PathBuf,
    PipeListener<pipe_mode::Bytes, pipe_mode::Bytes>,
) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir()
        .join(format!(
            "herdr-pipe-access-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
        .join("Windows 用户")
        .join("herdr-client.sock");
    let address = PathBuf::from(format!(r"\\.\pipe\{}", path.display()));
    let descriptor = SecurityDescriptor::deserialize(&sddl.to_wtf_16().unwrap()).unwrap();
    let listener = PipeListenerOptions::new()
        .path(address.as_path())
        .accept_remote(false)
        .nonblocking(true)
        .security_descriptor(Some(descriptor))
        .create_duplex::<pipe_mode::Bytes>()
        .unwrap();
    (path, address, listener)
}

#[test]
fn read_write_dacl_allows_the_daemon_connector_and_duplex_io() {
    let sid = crate::private_endpoint::user_sid().unwrap();
    // No Everyone, administrator, or full-control grant: exercise exactly the
    // generic read/write rights requested by both interprocess and .NET InOut.
    round_trip(&format!("O:{sid}D:P(A;;GRGW;;;{sid})"));
}

#[test]
fn daemon_owner_rights_dacl_allows_the_connector_and_duplex_io() {
    // Herdr 0.9.3's bind_private_local_listener uses this descriptor. OW is
    // OWNER RIGHTS, distinct from a grant to the token's user SID above.
    round_trip("D:P(A;;GA;;;SY)(A;;GA;;;OW)");
}

fn round_trip(sddl: &str) {
    let (path, _, listener) = listener(sddl);
    let mut client = Stream::connect(&path).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    // Nonblocking accept/read make a broken connection fail rather than hang.
    let mut server = listener.accept().unwrap();
    client.write_all(b"client").unwrap();
    client.flush().unwrap();
    assert_eq!(pipe_available(server.as_handle()).unwrap(), Some(6));
    let mut request = [0; 6];
    server.read_exact(&mut request).unwrap();
    assert_eq!(&request, b"client");
    server.write_all(b"daemon").unwrap();
    let mut response = [0; 6];
    client.read_exact(&mut response).unwrap();
    assert_eq!(&response, b"daemon");
    assert!(!path.exists());
}

#[test]
fn denied_write_preserves_access_denied_without_read_only_fallback() {
    let sid = crate::private_endpoint::user_sid().unwrap();
    // Deny only FILE_WRITE_DATA, not reading or creating a pipe instance.
    let sddl = format!("O:{sid}D:P(D;;0x00000002;;;{sid})(A;;GRGW;;;{sid})");
    let (path, address, _listener) = listener(&sddl);
    let error = Stream::connect(&path).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert_eq!(error.raw_os_error(), Some(5));
    // The name exists and read access works. A duplex client must still refuse
    // it rather than masking the denial or silently opening a one-way stream.
    let _read_only = OpenOptions::new().read(true).open(address).unwrap();
}
