//! The Unix endpoint: a socket file in a private directory.

use std::{
    fs,
    io::{self, Read, Write},
    net::Shutdown,
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::Path,
    time::Duration,
};

pub struct Listener(UnixListener);

impl Listener {
    /// Listens at `address`, making its directory private to the user. An
    /// existing file there is left alone and the bind fails.
    pub fn bind(address: &Path) -> io::Result<Self> {
        if let Some(parent) = address.parent() {
            fs::create_dir_all(parent)?;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }
        let listener = UnixListener::bind(address)?;
        fs::set_permissions(address, fs::Permissions::from_mode(0o600))?;
        Ok(Self(listener))
    }

    pub fn accept(&self) -> io::Result<Connection> {
        Ok(Connection(self.0.accept()?.0))
    }
}

pub(super) fn release(address: &Path) {
    let _ = fs::remove_file(address);
}

pub struct Connection(UnixStream);

impl Connection {
    /// Connects to `address`. A socket connects at once or not at all, so
    /// `timeout` only bounds a pipe on Windows.
    pub fn connect(address: &Path, _timeout: Duration) -> io::Result<Self> {
        Ok(Self(UnixStream::connect(address)?))
    }

    pub fn split(self) -> io::Result<(Reader, Writer, Closer)> {
        Ok((
            Reader(self.0.try_clone()?),
            Writer(self.0.try_clone()?),
            Closer(self.0),
        ))
    }
}

pub struct Reader(UnixStream);

impl Reader {
    /// Bounds each read; `None` waits as long as it takes.
    pub fn set_timeout(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        self.0.set_read_timeout(timeout)
    }
}

impl Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

pub struct Writer(UnixStream);

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

pub struct Closer(UnixStream);

impl Closer {
    /// Ends the connection both ways: the reader sees the end of the stream
    /// and further writes fail.
    pub fn close(&self) {
        let _ = self.0.shutdown(Shutdown::Both);
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(self.0.try_clone()?))
    }
}
