use super::*;
use crate::private_endpoint::Listener;
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
    thread,
};

fn pipe() -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    PathBuf::from(format!(
        r"\\.\pipe\herdr-owned-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn an_owned_connect_reaches_a_pipe_this_user_serves() {
    let path = pipe();
    let listener = Listener::bind(&path).unwrap();
    let dialed = path.clone();
    let client = thread::spawn(move || Stream::connect_owned(&dialed).unwrap());
    let (_reader, mut writer, _closer) = listener.accept().unwrap().split().unwrap();
    let mut client = client.join().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    writer.write_all(b"owned").unwrap();
    let mut read = [0; 5];
    client.read_exact(&mut read).unwrap();
    assert_eq!(&read, b"owned");
}

#[test]
fn an_owned_connect_fails_when_nothing_serves_the_pipe() {
    assert!(Stream::connect_owned(pipe()).is_err());
}
