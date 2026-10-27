//! Saved GitLab tokens share one record, so windows saving at once take
//! turns: each read, change, and write of it finishes before the next.
#![cfg(unix)]
use crate::gitlab::saved::{MAX_HOSTS, read_in, save_in};
use herdr_credentials::Store;
use secrecy::SecretString;
use std::sync::{Arc, Barrier};

#[test]
fn saves_made_at_once_keep_every_host() {
    let dir = tempfile::tempdir().unwrap();
    let start = Arc::new(Barrier::new(MAX_HOSTS));
    let saves: Vec<_> = (0..MAX_HOSTS)
        .map(|index| {
            let path = dir.path().to_owned();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                save_in(
                    Store::File,
                    &path,
                    &format!("gitlab{index}.example.com"),
                    Some(SecretString::from("glpat-x")),
                )
            })
        })
        .collect();
    for save in saves {
        save.join().unwrap().unwrap();
    }
    assert_eq!(
        read_in(Store::File, dir.path()).unwrap().hosts().count(),
        MAX_HOSTS
    );
}
