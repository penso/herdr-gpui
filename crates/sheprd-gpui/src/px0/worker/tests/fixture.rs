//! A native stand-in for px0, entered only by the owned-process test's child.
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::PathBuf,
    time::Duration,
};

#[test]
fn workbench() -> anyhow::Result<()> {
    if std::env::var_os("HERDR_PX0_TEST_CHILD").is_none() {
        return Ok(());
    }
    let state = PathBuf::from(
        std::env::var_os("XDG_STATE_HOME")
            .ok_or_else(|| anyhow::anyhow!("missing fixture state"))?,
    )
    .join("px0");
    std::fs::create_dir_all(&state)?;
    let server = TcpListener::bind("127.0.0.1:0")?;
    let url = format!("http://{}/", server.local_addr()?);
    std::fs::write(
        state.join("workbench.lock"),
        serde_json::to_vec(&serde_json::json!({"pid": std::process::id(), "url": url}))?,
    )?;
    for socket in server.incoming() {
        let mut socket = socket?;
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        let mut reader = BufReader::new(&socket);
        let mut line = String::new();
        while reader.read_line(&mut line)? != 0 && !line.ends_with("\r\n\r\n") && line.len() < 8192
        {
        }
        let body = r#"{"workbench":true,"version":"fixture"}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )?;
    }
    Ok(())
}
