//! A pinned OpenSSH command for one Boat sandbox. The app owns one persistent
//! private key; Boat authorizes its public half on each running machine and
//! returns that machine's host key over the authenticated API connection.

use super::{Error, Result, api::SshAccess};
use crate::cloud::Value;
use std::{
    io::Write,
    net::IpAddr,
    path::Path,
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
};

const ALIAS: &str = "herdr-boat-sandbox";
mod key;
static KEY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

// OpenSSH expands percent tokens and parses whitespace even in an argv value.
fn quoted_path(path: &Path) -> Result<String> {
    let text = path.to_str().ok_or(Error::Gateway)?;
    if !path.is_absolute() || text.contains(['"', '\n', '\r', '\0']) {
        return Err(Error::Gateway);
    }
    Ok(format!("\"{}\"", text.replace('%', "%%")))
}

fn private_key() -> Result<key::Identity> {
    let _guard = KEY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let directory = herdr_client::local_path::state_dir()
        .ok_or(Error::Cloud(crate::cloud::Error::StateDirectory))?;
    key::snapshot(&directory, |generated| {
        let status = herdr_client::new_command("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(generated)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(Error::SshMissing)?;
        if !status.success() {
            return Err(Error::SshKeygen);
        }
        Ok(())
    })
}

fn public_key(private: &Path) -> Result<String> {
    let output = herdr_client::new_command("ssh-keygen")
        .args(["-y", "-f"])
        .arg(private)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(Error::SshMissing)?;
    if !output.status.success() {
        return Err(Error::SshKeygen);
    }
    let key =
        String::from_utf8(output.stdout).map_err(|_| Error::Encoding(Value::BoatSshPublicKey))?;
    let key = key.trim();
    if key.len() > 4096
        || !key.starts_with("ssh-ed25519 ")
        || key.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(Error::Gateway);
    }
    Ok(key.to_owned())
}

fn user(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn host_key(value: &str) -> bool {
    let Some((kind, data)) = value.split_once(' ') else {
        return false;
    };
    matches!(kind, "ssh-ed25519" | "ecdsa-sha2-nistp256" | "ssh-rsa")
        && !data.is_empty()
        && data.len() <= 4096
        && data
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
}

fn endpoint(access: &SshAccess) -> Result<(String, u16)> {
    if let Some(endpoint) = &access.ssh_endpoint {
        let (host, port) = endpoint.rsplit_once(':').ok_or(Error::Gateway)?;
        let host: IpAddr = host.parse().map_err(|_| Error::Gateway)?;
        let port = port.parse().map_err(|_| Error::Gateway)?;
        if port == 0 {
            return Err(Error::Gateway);
        }
        return Ok((host.to_string(), port));
    }
    let host: IpAddr = access
        .machine_ip
        .as_deref()
        .ok_or(Error::Gateway)?
        .parse()
        .map_err(|_| Error::Gateway)?;
    Ok((host.to_string(), 22))
}

pub struct Gateway {
    pub command: Command,
    _known_hosts: tempfile::NamedTempFile,
    _known_directory: tempfile::TempDir,
    _identity: Option<key::Identity>,
}

impl Gateway {
    /// Keep the pinned host-key file alive until the operation finishes.
    pub fn with_command<T>(self, run: impl FnOnce(Command) -> T) -> T {
        run(self.command)
    }

    pub fn new(access: &SshAccess, private: &Path) -> Result<Self> {
        if !user(&access.ssh_user) || !host_key(&access.host_key) {
            return Err(Error::Gateway);
        }
        let (host, port) = endpoint(access)?;
        let known_directory = key::temporary()?;
        let mut known_hosts =
            tempfile::NamedTempFile::new_in(known_directory.path()).map_err(Error::GatewayFile)?;
        writeln!(known_hosts, "{ALIAS} {}", access.host_key)
            .and_then(|()| known_hosts.flush())
            .map_err(Error::GatewayFile)?;
        let known_hosts_path = quoted_path(known_hosts.path())?;
        let mut command = herdr_client::new_command("ssh");
        command
            .args(["-F", "/dev/null", "-T"])
            .arg("-o")
            .arg(format!("IdentityFile={}", quoted_path(private)?))
            .args(["-o", "IdentitiesOnly=yes"])
            .args(["-o", "BatchMode=yes"])
            .args(["-o", "NumberOfPasswordPrompts=0"])
            .args(["-o", "StrictHostKeyChecking=yes"])
            .arg("-o")
            .arg(format!("UserKnownHostsFile={known_hosts_path}"))
            .args(["-o", "GlobalKnownHostsFile=/dev/null"])
            .args(["-o", &format!("HostKeyAlias={ALIAS}")])
            .args(["-o", "ConnectTimeout=15"])
            .args(["-o", "ConnectionAttempts=1"])
            .args(["-o", "ServerAliveInterval=15"])
            .args(["-o", "ServerAliveCountMax=4"])
            .args(["-o", "ForwardAgent=no"])
            .args(["-o", "ForwardX11=no"])
            .args(["-o", "ClearAllForwardings=yes"])
            .args(["-o", "ControlMaster=no"])
            .args(["-o", "LogLevel=ERROR"])
            .args(["-p", &port.to_string(), "-l", &access.ssh_user, "--"])
            .arg(host);
        Ok(Self {
            command,
            _known_hosts: known_hosts,
            _known_directory: known_directory,
            _identity: None,
        })
    }
}

pub fn prepare(request: &super::http::Request, id: &str) -> Result<Gateway> {
    let private = private_key()?;
    let public = public_key(private.path())?;
    let mut gateway = Gateway::new(&request.ssh_access(id, &public)?, private.path())?;
    gateway._identity = Some(private);
    Ok(gateway)
}

pub fn check_ssh() -> Result<()> {
    for program in ["ssh", "ssh-keygen"] {
        herdr_client::new_command(program)
            .arg("-V")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(drop)
            .map_err(Error::SshMissing)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
