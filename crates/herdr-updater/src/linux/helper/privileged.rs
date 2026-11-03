//! The sole elevation boundary. User-owned paths are read through pinned file
//! descriptors; only authenticated bytes in private root-owned storage reach
//! a package manager. Nothing in the request chooses an executable or command.
use super::*;
use rustix::fs::{Mode, OFlags, openat};
use std::{fs::File, os::unix::fs::MetadataExt, path::Component};

const STORAGE: &str = "/var/cache";

/// Traverse without following any links, holding each directory open while
/// opening its child. Checking paths with lstat then reopening would race.
fn directory(path: &Path, trusted: bool) -> Result<File> {
    if !path.is_absolute() {
        return Err(Error::RelativeInstallation);
    }
    let mut directory = File::open("/")?;
    for component in path.components().skip(1) {
        let Component::Normal(name) = component else {
            return Err(Error::NoncanonicalInstallation);
        };
        directory = File::from(
            openat(
                &directory,
                name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?,
        );
        if trusted {
            let meta = directory.metadata()?;
            if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
                return Err(Error::UnsafePackageHelper);
            }
        }
    }
    Ok(directory)
}

pub(super) fn trusted_executable() -> Result<()> {
    let parent = directory(Path::new("/usr/bin"), true)?;
    let file = File::from(
        openat(
            &parent,
            "herdr-gpui",
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.uid() != 0
        || meta.nlink() != 1
        || meta.mode() & 0o6022 != 0
        || meta.mode() & 0o111 == 0
    {
        return Err(Error::UnsafePackageHelper);
    }
    Ok(())
}

pub(super) fn open_stage(path: &Path, uid: u32) -> Result<File> {
    let stage = directory(path, false)?;
    let meta = stage.metadata()?;
    if meta.uid() != uid || meta.mode() & 0o7777 != 0o700 {
        return Err(Error::StagePermissions);
    }
    Ok(stage)
}

pub(super) fn regular_file(stage: &File, name: &str, uid: u32) -> Result<File> {
    // `name` is either request.json or an authenticated manifest asset name.
    let file = File::from(
        openat(
            stage,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != uid || meta.nlink() != 1 || meta.mode() & 0o7777 != 0o600 {
        return Err(Error::StagePermissions);
    }
    Ok(file)
}

/// The production caller has required euid 0 and trusted STORAGE's ancestors.
/// No subprocess runs until the private copy has passed its signed size/hash.
fn with_protected_package(
    source: impl Read,
    asset: &release::Asset,
    parent: &Path,
    install: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let stage = tempfile::Builder::new()
        .prefix(".herdr-package-")
        .tempdir_in(parent)?;
    fs::set_permissions(stage.path(), fs::Permissions::from_mode(0o700))?;
    let path = stage.path().join(&asset.name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    release::copy_archive(source, &mut file, asset, &AtomicBool::new(false), |_, _| {})?;
    file.sync_all()?;
    drop(file);
    install(&path)
}

pub(super) fn apply_request(stage: &Path, key: &str) -> Result<()> {
    if !rustix::process::geteuid().is_root() {
        return Err(Error::PackageElevation);
    }
    let uid = env::var("PKEXEC_UID")
        .ok()
        .and_then(|uid| uid.parse::<u32>().ok())
        .filter(|uid| *uid != 0)
        .ok_or(Error::PackageElevation)?;
    if env::current_exe()? != Path::new(SYSTEM_EXECUTABLE) {
        return Err(Error::UnsafePackageHelper);
    }
    trusted_executable()?;
    let source_stage = open_stage(stage, uid)?;
    // Read and authenticate again AFTER authorization. The unprivileged
    // process's earlier checks cannot vouch for bytes it can still replace.
    let (request, manifest) = decode_request(
        regular_file(&source_stage, "request.json", uid)?,
        key,
        crate::APP_VERSION,
    )?;
    let target = release::target().ok_or(Error::UnsupportedTarget)?;
    let asset = request.format.select(&manifest, target)?;
    let cancel = AtomicBool::new(false);
    let package = locate(Path::new(SYSTEM_EXECUTABLE), |command| {
        install::output(command, &cancel)
    })?;
    if package.format != request.format {
        return Err(Error::PackageChanged);
    }
    // /var/cache and every ancestor must be root-owned and unwriteable to
    // other users. TMPDIR, HOME and request paths never choose root storage.
    let _parent = directory(Path::new(STORAGE), true)?;
    let source = regular_file(&source_stage, &asset.name, uid)?;
    with_protected_package(source, asset, Path::new(STORAGE), |path| {
        transact(install_command(package.format, path), |detail| {
            let _ = writeln!(std::io::stdout(), "{detail}");
        })
    })?;
    if installed_version(package.format, &cancel)? != format!("{}-1", manifest.version) {
        return Err(Error::PackageVersion);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
