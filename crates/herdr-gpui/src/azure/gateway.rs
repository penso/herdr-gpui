//! SSH key generation and OpenSSH command builder for Azure Virtual Machines.

use super::{Error, Result};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, OnceLock},
};

static KEY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const KEY_NAME: &str = "azure_id_ed25519";
const KNOWN_HOSTS_NAME: &str = "azure_known_hosts";

const POLICY: [&str; 20] = [
    "AddressFamily=inet",
    "BatchMode=yes",
    "NumberOfPasswordPrompts=0",
    "PreferredAuthentications=publickey",
    "PasswordAuthentication=no",
    "IdentitiesOnly=yes",
    "StrictHostKeyChecking=accept-new",
    "VerifyHostKeyDNS=no",
    "GlobalKnownHostsFile=/dev/null",
    "ConnectTimeout=15",
    "ConnectionAttempts=1",
    "ServerAliveInterval=15",
    "ServerAliveCountMax=4",
    "ForwardAgent=no",
    "ForwardX11=no",
    "ClearAllForwardings=yes",
    "ControlMaster=no",
    "ControlPath=none",
    "ProxyCommand=none",
    "LogLevel=ERROR",
];

pub(crate) struct Gateway {
    identity_path: PathBuf,
    known_hosts_path: PathBuf,
    user: String,
    ip: String,
}

impl Gateway {
    pub(crate) fn new(user: &str, ip: &str) -> Result<Self> {
        let (identity_path, _) = persistent_key()?;
        let directory = crate::preferences::state_dir()
            .ok_or(Error::Cloud(crate::cloud::Error::StateDirectory))?;
        let known_hosts_path = directory.join(KNOWN_HOSTS_NAME);

        Ok(Self {
            identity_path,
            known_hosts_path,
            user: user.to_owned(),
            ip: ip.to_owned(),
        })
    }

    pub(crate) fn build_command(&self) -> Result<Command> {
        let mut command = herdr_client::new_command("ssh");
        command.arg("-F").arg("/dev/null");
        command.arg("-T");

        let identity = format!("IdentityFile={}", quoted_path(&self.identity_path)?);
        let known_hosts = format!(
            "UserKnownHostsFile={}",
            quoted_path(&self.known_hosts_path)?
        );

        command.args(["-o", &identity]);
        command.args(["-o", &known_hosts]);

        for option in POLICY {
            command.args(["-o", option]);
        }

        command.arg(format!("{}@{}", self.user, self.ip));
        Ok(command)
    }

    pub(crate) fn with_command<T>(&self, run: impl FnOnce(Command) -> T) -> Result<T> {
        let command = self.build_command()?;
        Ok(run(command))
    }
}

pub(crate) fn persistent_key() -> Result<(PathBuf, String)> {
    let _guard = KEY_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    let directory =
        crate::preferences::state_dir().ok_or(Error::Cloud(crate::cloud::Error::StateDirectory))?;
    std::fs::create_dir_all(&directory).map_err(Error::Process)?;

    let private_path = directory.join(KEY_NAME);
    let public_path = directory.join(format!("{}.pub", KEY_NAME));

    if !private_path.exists() {
        let status = herdr_client::new_command("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(&private_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(Error::SshMissing)?;

        if !status.success() {
            return Err(Error::SshKeygen);
        }
    }

    let public_key = if public_path.exists() {
        std::fs::read_to_string(&public_path).map_err(Error::Process)?
    } else {
        read_public_key(&private_path)?
    };

    let key = public_key.trim().to_owned();
    if !key.starts_with("ssh-ed25519 ") {
        return Err(Error::Gateway);
    }

    Ok((private_path, key))
}

fn read_public_key(private: &Path) -> Result<String> {
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

    String::from_utf8(output.stdout).map_err(|_| Error::Encoding("Azure SSH public key"))
}

fn quoted_path(path: &Path) -> Result<String> {
    let text = path.to_str().ok_or(Error::Gateway)?;
    if !path.is_absolute() || text.contains(['"', '\n', '\r', '\0']) {
        return Err(Error::Gateway);
    }
    Ok(format!("\"{}\"", text.replace('%', "%%")))
}
