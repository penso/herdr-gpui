//! Own the package transaction outside the GUI's lifetime. The non-elevated
//! owner drains progress; pkexec invokes the fixed installed executable to
//! authenticate and install a root-owned copy after authorization.
use super::*;
use std::{
    ffi::OsString, fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, process::ExitCode,
};

pub(super) const FLAG: &str = "--herdr-install-package";
const ROOT_FLAG: &str = "--herdr-install-package-root";
const LIMIT: u64 = 128 * 1024;

mod privileged;

pub(super) fn elevated_command(stage: &Path) -> Result<Command> {
    privileged::trusted_executable()?;
    Ok(authorization_command(stage))
}

fn authorization_command(stage: &Path) -> Command {
    let mut command = sheprd_client::new_command("/usr/bin/pkexec");
    command
        .args([SYSTEM_EXECUTABLE, ROOT_FLAG])
        .arg(stage)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LC_ALL", "C")
        .current_dir("/")
        .stdin(Stdio::null());
    command
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    executable: PathBuf,
    format: Format,
    version: String,
    manifest: Vec<u8>,
    signature: Vec<u8>,
}

pub(super) fn write_request(prepared: &Prepared) -> Result<()> {
    let request = Request {
        executable: prepared.package.executable.clone(),
        format: prepared.package.format,
        version: prepared.offer.manifest.version.clone(),
        manifest: prepared.offer.manifest_bytes.clone(),
        signature: prepared.offer.signature.clone(),
    };
    let bytes = serde_json::to_vec(&request)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::RequestLimit);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(prepared.stage.path().join("request.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn request(stage: &Path, key: &str, current: &str) -> Result<(Request, release::Manifest)> {
    let uid = install::effective_uid()?;
    if uid == 0 {
        return Err(Error::RootUser);
    }
    let stage = privileged::open_stage(stage, uid)?;
    decode_request(
        privileged::regular_file(&stage, "request.json", uid)?,
        key,
        current,
    )
}

fn decode_request(
    reader: impl Read,
    key: &str,
    current: &str,
) -> Result<(Request, release::Manifest)> {
    let mut bytes = Vec::new();
    reader.take(LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::RequestLimit);
    }
    let request: Request = serde_json::from_slice(&bytes)?;
    // This is the package-owned GUI identity, never the executable elevated by
    // PolicyKit. That verifier remains fixed at SYSTEM_EXECUTABLE.
    if matches!(request.format, Format::Archive | Format::Installer)
        || (request.executable.as_os_str() != SYSTEM_EXECUTABLE
            && request.executable.as_os_str() != "/usr/bin/sheprd")
        || release::parse_version(current).is_none()
        || release::parse_version(&request.version) <= release::parse_version(current)
    {
        return Err(Error::StaleRequest);
    }
    let manifest =
        release::verify_manifest(&request.manifest, &request.signature, key, &request.version)?;
    if manifest.schema != 2 {
        return Err(Error::ManifestSchema);
    }
    Ok((request, manifest))
}

fn apply_request(stage: &Path, key: &str) -> Result<()> {
    let (request, manifest) = request(stage, key, crate::APP_VERSION)?;
    let target = release::target().ok_or(Error::UnsupportedTarget)?;
    let asset = request.format.select(&manifest, target)?.clone();
    let package = Package {
        format: request.format,
        executable: request.executable,
    };
    let offer = release::Offer {
        manifest,
        asset,
        manifest_bytes: request.manifest,
        signature: request.signature,
        notes: String::new(),
    };
    let result = apply(
        &package,
        &offer,
        stage,
        key,
        &AtomicBool::new(false),
        |detail| {
            // A closed GUI must never turn progress output into a panic/SIGPIPE.
            let _ = writeln!(std::io::stdout(), "{detail}");
        },
    );
    // Remove only our known files, never recursively erase an argument path.
    let _ = fs::remove_file(stage.join(&offer.asset.name));
    let _ = fs::remove_file(stage.join("request.json"));
    let _ = fs::remove_dir(stage);
    result
}

#[cfg(test)]
mod tests;

pub(crate) fn run_helper(args: &[OsString]) -> Option<ExitCode> {
    if args
        .first()
        .is_none_or(|arg| arg != FLAG && arg != ROOT_FLAG)
    {
        return None;
    }
    let result = match args {
        [flag, stage] => option_env!("HERDR_UPDATE_PUBLIC_KEY")
            .ok_or(Error::LocalBuild)
            .and_then(|key| {
                if flag == ROOT_FLAG {
                    privileged::apply_request(Path::new(stage), key)
                } else {
                    apply_request(Path::new(stage), key)
                }
            }),
        _ => Err(Error::MalformedRequest),
    };
    Some(match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "{error}");
            ExitCode::FAILURE
        }
    })
}
