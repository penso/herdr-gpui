//! Owner-only directories for the control sockets of the SSH masters this
//! crate starts: one per port forward, and the shared ones in `ssh::mux`.
use std::{
    fs::DirBuilder,
    io,
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// A fresh owner-only directory for one master's control socket, removed
/// with everything in it once dropped, which its owner does only after the
/// master is reaped. Creation fails rather than reuse anything already at
/// the path.
#[derive(Debug)]
pub(crate) struct ControlDir(PathBuf);

impl ControlDir {
    /// `prefix` names who made it; the process ID and a counter make it
    /// unique.
    pub(crate) fn create(prefix: &str) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let name = format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = Self::place(&std::env::temp_dir(), &name);
        DirBuilder::new().mode(0o700).create(&path)?;
        Ok(Self(path))
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }

    /// Where SSH binds the control socket.
    pub(crate) fn socket(&self) -> PathBuf {
        self.0.join("c")
    }

    /// Where to create the directory `name`: the temporary directory,
    /// unless the socket SSH binds there would not fit. SSH binds it under a
    /// 17-byte random suffix first, and Unix socket paths are limited to 104
    /// bytes on macOS (108 on Linux).
    pub(crate) fn place(temp: &Path, name: &str) -> PathBuf {
        let path = temp.join(name);
        if path.join("c").as_os_str().len() + 17 < 104 {
            path
        } else {
            Path::new("/tmp").join(name)
        }
    }
}

/// The `-o` value naming `socket` as a master's control socket. SSH expands
/// `%` tokens in a `ControlPath`, so a literal one is doubled.
pub(crate) fn control_path(socket: &Path) -> String {
    format!(
        "ControlPath={}",
        socket.to_string_lossy().replace('%', "%%")
    )
}

impl Drop for ControlDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
