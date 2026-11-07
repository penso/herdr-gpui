//! Repositories on this machine, read with gix rather than by starting `git`:
//! what `repo::detect`, the agent branch listing, and the base commit need.
//! Removing a worktree still runs `git`, which owns that mutation.

use super::{Error, Result};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// What `repo::detect`'s script prints for a checkout.
pub struct Checkout {
    /// The git common directory, canonicalized as `pwd -P` resolves it.
    pub common: PathBuf,
    /// The main checkout, else this one when the main repository is bare.
    pub main: PathBuf,
    /// Every remote and its fetch URL with `insteadOf` rewrites applied, as
    /// `git remote get-url` prints it; empty when it has none.
    pub remotes: Vec<(String, String)>,
}

fn git(operation: &'static str) -> impl FnOnce(Box<dyn std::error::Error + Send + Sync>) -> Error {
    move |source| Error::Git { operation, source }
}

/// The configuration sources outside the repository, in Git's order.
const GLOBALS: [gix::config::Source; 4] = [
    gix::config::Source::GitInstallation,
    gix::config::Source::System,
    gix::config::Source::Git,
    gix::config::Source::User,
];

/// The repository holding `path`, opened without the configuration outside
/// it: gix would find that through this process's environment, which a
/// desktop launch may lack. [`with_globals`] adds it back.
fn discover(path: &Path) -> gix::Result<gix::Repository> {
    let mut trust = gix::sec::trust::Mapping::<gix::open::Options>::default();
    for options in [&mut trust.full, &mut trust.reduced] {
        let config = &mut options.permissions.config;
        config.git_binary = false;
        config.system = false;
        config.git = false;
        config.user = false;
    }
    gix::ThreadSafeRepository::discover_opts(path, Default::default(), trust).map(Into::into)
}

/// Puts the installation, system, and global configuration before `repo`'s
/// own, found and expanded through `env`, the user's login environment as
/// the `git` this replaced saw it, so `includeIf` and `url.insteadOf` there
/// apply.
fn with_globals(
    repo: &mut gix::Repository,
    env: &mut dyn FnMut(&str) -> Option<OsString>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use gix::config::{File, file};
    let home = env("HOME")
        .map(PathBuf::from)
        .or_else(gix::path::env::home_dir);
    let mut metas = GLOBALS.into_iter().filter_map(|source| {
        source.storage_location(&mut *env).map(|path| {
            file::Metadata::from(source)
                .at(path)
                .with(gix::sec::Trust::Full)
        })
    });
    // `includeIf "gitdir:"` matches Git's real path. gix resolves symlinks
    // but not Windows 8.3 short names, such as a runner's `RUNNER~1` temp
    // folder, so it gets the canonical path `git` would compare.
    let git_dir = repo
        .git_dir()
        .canonicalize()
        .map(super::repo::plain)
        .unwrap_or_else(|_| repo.git_dir().to_owned());
    let branch = repo.head_name().ok().flatten();
    let options = file::init::Options {
        includes: file::includes::Options::follow(
            gix::config::path::interpolate::Context {
                git_install_dir: None,
                home_dir: home.as_deref(),
                home_for_user: Some(gix::config::path::interpolate::home_for_user),
            },
            file::includes::conditional::Context {
                git_dir: Some(&git_dir),
                branch_name: branch.as_ref().map(AsRef::as_ref),
            },
        ),
        ..Default::default()
    };
    let mut buf = Vec::new();
    let no_includes = file::init::Options {
        includes: file::includes::Options::no_follow(),
        ..Default::default()
    };
    let Some(mut globals) =
        File::from_paths_metadata_buf(&mut metas, &mut buf, false, no_includes)?
    else {
        return Ok(());
    };
    globals.resolve_includes(options)?;
    let mut config = repo.config_snapshot_mut();
    let local = std::mem::take(&mut *config);
    let meta = local.meta_owned();
    globals.append(local)?;
    globals.set_meta(meta);
    *config = globals;
    config.commit()?;
    Ok(())
}

/// The repository holding `checkout`, or `None` when it is not in one.
pub fn checkout(checkout: &Path) -> Result<Option<Checkout>> {
    checkout_in(checkout, &mut |name| sheprd_client::login_env::var(name))
}

/// [`checkout`] with Git's configuration found through `env`.
pub fn checkout_in(
    checkout: &Path,
    env: &mut dyn FnMut(&str) -> Option<OsString>,
) -> Result<Option<Checkout>> {
    const READING: &str = "Reading the repository";
    use gix::discover::upwards::Error as Upwards;
    // A missing folder or one outside Git is "not a repository", as `git
    // rev-parse` failing was; anything else is a failure worth showing.
    if !checkout.is_dir() {
        return Ok(None);
    }
    let mut repo = match discover(checkout) {
        Ok(repo) => repo,
        Err(error)
            if matches!(
                error.downcast_any_ref::<Upwards>(),
                Some(
                    Upwards::NoGitRepository { .. }
                        | Upwards::NoGitRepositoryWithinCeiling { .. }
                        | Upwards::NoGitRepositoryWithinFs { .. }
                )
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(git(READING)(error.into())),
    };
    with_globals(&mut repo, env).map_err(git(READING))?;
    let Some(root) = repo.workdir().map(Path::to_path_buf) else {
        // A bare repository has no checkout to list issues for.
        return Ok(None);
    };
    let common = super::repo::plain(
        repo.common_dir()
            .canonicalize()
            .map_err(|error| git(READING)(error.into()))?,
    );
    let main_repo = repo
        .main_repo()
        .map_err(|error| git(READING)(error.into()))?;
    let main = main_repo.workdir().map_or(root, Path::to_path_buf);
    // Remotes live in the shared config, which a linked worktree reads too.
    let remotes = repo
        .remote_names()
        .into_iter()
        .map(|name| {
            let url = repo
                .find_remote(name.as_slice())
                .ok()
                .and_then(|remote| {
                    remote
                        .url(gix::remote::Direction::Fetch)
                        .map(|url| url.to_bstring().to_string())
                })
                .unwrap_or_default();
            (name.to_string(), url)
        })
        .collect();
    Ok(Some(Checkout {
        common,
        main,
        remotes,
    }))
}

/// Up to `max` branch names under `refs/heads/{prefix}`, short, in name
/// order, as `git for-each-ref --format=%(refname:short)` lists them.
pub fn branches(main_root: &Path, prefix: &str, max: usize) -> Result<Vec<String>> {
    const READING: &str = "Reading the branches";
    let repo = gix::open(main_root).map_err(|error| git(READING)(error.into()))?;
    let references = repo
        .references()
        .map_err(|error| git(READING)(error.into()))?;
    let full = format!("refs/heads/{prefix}");
    // A branch that cannot be read fails the listing, as `git for-each-ref`
    // would, rather than vanishing from a list that looks complete.
    references
        .prefixed(full.as_str())
        .map_err(|error| git(READING)(error.into()))?
        .take(max)
        .map(|reference| {
            reference
                .map(|reference| reference.name().shorten().to_string())
                .map_err(|error| git(READING)(error.into()))
        })
        .collect()
}

/// The full commit id `base` names in `main_root`: a reference such as
/// `HEAD` or a branch, peeled through tags, else a full commit id.
pub fn commit(main_root: &Path, base: &str) -> Result<String> {
    const READING: &str = "Reading the base commit";
    let repo = gix::open(main_root).map_err(|error| git(READING)(error.into()))?;
    let found = match repo
        .try_find_reference(base)
        .map_err(|error| git(READING)(error.into()))?
    {
        Some(mut reference) => {
            reference
                .peel_to_commit()
                .map_err(|error| git(READING)(error.into()))?
                .id
        }
        None => {
            let id = gix::ObjectId::from_hex(base.as_bytes())
                .map_err(|error| git(READING)(error.into()))?;
            repo.find_object(id)
                .map_err(|error| git(READING)(error.into()))?
                .peel_to_commit()
                .map_err(|error| git(READING)(error.into()))?
                .id
        }
    };
    Ok(found.to_string())
}
