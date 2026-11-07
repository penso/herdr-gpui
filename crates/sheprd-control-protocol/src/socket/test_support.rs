//! Socket addresses for tests here and in the app's server tests, which
//! reach this through the `test-support` feature.
#![allow(clippy::unwrap_used)]
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::atomic::Ordering;

/// A private address, removed with the guard where it is a file.
pub struct Address {
    pub path: PathBuf,
    #[cfg(unix)]
    _dir: tempfile::TempDir,
}

/// A fresh address no other test uses.
#[cfg(unix)]
pub fn address() -> Address {
    // Short: socket paths are limited to about 100 bytes.
    let dir = tempfile::Builder::new()
        .prefix("hgc")
        .tempdir_in("/tmp")
        .unwrap();
    Address {
        path: dir.path().join("control.sock"),
        _dir: dir,
    }
}

/// A fresh address no other test uses.
#[cfg(windows)]
pub fn address() -> Address {
    use std::sync::atomic::AtomicU64;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    Address {
        path: PathBuf::from(format!(
            r"\\.\pipe\herdr-gpui-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )),
    }
}
