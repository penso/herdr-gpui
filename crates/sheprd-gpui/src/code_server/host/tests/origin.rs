//! Each host's pages load its server at a name of their own, so no server's
//! token cookie goes to another: browsers keep cookies by name, not port.
use super::*;
use crate::browser::WebUrl;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

const COMMIT: &str = "2a59476c9bfcb90b3ddc372c36762471b7dfad1c";

#[test]
fn each_host_has_a_page_name_of_its_own_on_this_computer() {
    let (one, two) = (page_host("dev@build"), page_host("dev@other"));
    assert_ne!(one, two);
    assert_eq!(one, page_host("dev@build"), "the same each launch");
    for name in [&one, &two] {
        let page = WebUrl::try_from(format!("http://{name}:24567/?tkn=x")).unwrap();
        assert_eq!(page.host(), name.as_str());
        assert_ne!(page.host(), "127.0.0.1", "the local server's name");
        assert!(name.ends_with(".localhost"), "{name}");
        assert_eq!(page.loopback_port().map(NonZeroU16::get), Some(24_567));
    }
}

/// A page name the resolver sends to another machine is refused, so its
/// token never goes there; loopback and unresolved names are not.
#[test]
fn a_page_name_that_resolves_elsewhere_is_refused() {
    let port = NonZeroU16::new(24_567).unwrap();
    assert!(!resolves_elsewhere(&page_host("dev@build"), port));
    assert!(!resolves_elsewhere("127.0.0.1", port));
    assert!(!resolves_elsewhere("::1", port));
    // An address literal, which resolves without asking any DNS server.
    assert!(resolves_elsewhere("192.0.2.1", port));
    assert!(!resolves_elsewhere("vscode-test.invalid", port));
}

/// The version is asked on `127.0.0.1`, whatever this computer's resolver
/// makes of the page's name, and never with the token: by the worker and by
/// the windows, which both ask with `code_server::version`.
#[test]
fn a_hosts_server_is_asked_on_ipv4_loopback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let asked = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        let read = stream.read(&mut request).unwrap();
        stream
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: 40\r\nconnection: close\r\n\r\n{COMMIT}"
                )
                .as_bytes(),
            )
            .unwrap();
        String::from_utf8_lossy(&request[..read]).into_owned()
    });
    let page = WebUrl::try_from(format!(
        "http://{}:{port}/?tkn=secret",
        page_host("dev@build")
    ))
    .unwrap();
    let server = crate::code_server::version(&page).unwrap();
    assert_eq!(server.short_commit(), &COMMIT[..7]);
    let request = asked.join().unwrap();
    assert!(request.starts_with("GET /version "), "{request}");
    assert!(!request.contains("secret"), "{request}");
}

/// The address a host's worker publishes, which windows load, carries the
/// host's page name rather than `127.0.0.1`.
#[cfg(unix)]
#[test]
fn a_hosts_address_carries_its_page_name() {
    use crate::code_server::{Server, supervisor::Status};
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let plan = Plan {
        program: PathBuf::from("/bin/sh"),
        port: None,
        token_file: dir.path().join("vscode-token"),
        host: Some("dev@build".into()),
    };
    let server = Supervisor::start_with(
        plan,
        |_, _| {
            let mut command = sheprd_client::new_command("/bin/sh");
            command.args(["-c", "echo 'Web UI available'; exec sleep 60"]);
            Ok(command)
        },
        |_| Ok(Server::from_version(COMMIT).unwrap()),
        |_| Ok(()),
        Timing::default(),
    )
    .unwrap();
    let mut address = None;
    for _ in 0..500 {
        address = server.report().address;
        if address.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let address = address.unwrap();
    assert_eq!(address.url.host(), page_host("dev@build"));
    assert!(
        address.url.as_str().starts_with(&format!(
            "http://{}:{}/?tkn=",
            page_host("dev@build"),
            address.port
        )),
        "{:?}",
        address.url.origin()
    );
    assert!(!matches!(server.report().status, Status::Failed { .. }));
    server.shutdown(Duration::from_secs(1));
}
