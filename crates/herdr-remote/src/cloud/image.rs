//! Sandbox images built from each project's own recipe, so a new machine
//! starts with the project's tools and Herdr already in place. A project
//! keeps its recipe in `.herdr/sandbox/Dockerfile`, or a dev container
//! (see [`recipe`]); this app adds a layer installing Herdr, and files the
//! result by project and provider under a key hashed from everything the
//! build reads, so an unchanged recipe is reused and a changed one rebuilds.
//!
//! Providers that can hold images fill in a [`Support`] in their row of
//! `cloud::providers`; the rest keep installing Herdr at first start.

pub mod builds;
pub mod catalog;
mod files;
pub mod key;
pub mod progress;
pub mod recipe;

pub use key::Project;
pub use progress::Progress;

use std::{fmt, io, path::PathBuf, sync::atomic::AtomicBool};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Another build of this image is running; wait for it to finish.")]
    Busy,
    #[error("Cancelled.")]
    Cancelled,
    #[error("Could not update {}: {source}", path.display())]
    Catalog {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(
        "{0} cannot copy files into an image yet: remove the recipe's COPY and ADD lines, or use a container."
    )]
    Context(&'static str),
    #[error("The dev container definition is not valid JSON.")]
    Devcontainer(#[source] serde_json::Error),
    #[error("{0}")]
    Field(Field),
    #[error("Could not read the project's recipe from its host.")]
    Host(#[source] herdr_client::Error),
    #[error("Invalid sandbox image records.")]
    Json(#[source] serde_json::Error),
    #[error("The recipe names {0}, which the project does not have.")]
    Missing(String),
    #[error("No state directory for sandbox images.")]
    StateDirectory,
    #[error("The project's recipe has more files than an image build takes.")]
    TooLarge,
    #[error("The project's recipe changed while it was being read; try again.")]
    Truncated,
    #[error("This app has no record of that project's image.")]
    Unknown,
    #[error("This project's recipe uses {0}, which sandbox images do not support.")]
    Unsupported(Unsupported),
}

/// What a recipe or the image records hold that is refused, each a whole
/// sentence so the app can translate it as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    // keep-sorted start
    /// The image records file is too large or not this version's.
    Catalog,
    /// A build context file's path leaves the context.
    ContextPath,
    /// The dev container's `image` is not an image reference.
    DevcontainerImage,
    /// A build context file's name is not UTF-8.
    FileName,
    /// A recipe's path leaves the project.
    ProjectPath,
    /// The image records already hold as many projects as they take.
    Projects,
    // keep-sorted end
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog => write!(f, "{} is not valid.", catalog::FILE),
            Self::ContextPath => f.write_str("A file path outside the build context is not valid."),
            Self::DevcontainerImage => f.write_str("The dev container's image is not valid."),
            Self::FileName => f.write_str("A file name that is not UTF-8 is not valid."),
            Self::Projects => f.write_str("Sandbox images are kept for too many projects."),
            Self::ProjectPath => f.write_str("A path outside the project is not valid."),
        }
    }
}

/// A recipe this app cannot build an image from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unsupported {
    /// A dev container defined by Docker Compose.
    Compose,
    /// A dev container with neither an image nor a Dockerfile.
    NoBuild,
    /// A provider that cannot hold images, asked to build one.
    Provider,
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Compose => "a dev container defined by Docker Compose",
            Self::NoBuild => "a dev container with neither an image nor a Dockerfile",
            Self::Provider => "a provider that does not hold images",
        })
    }
}

/// A project's recipe, read from its host, ready to build.
#[derive(Clone, Debug)]
pub struct Recipe {
    pub project: Project,
    /// Where it was read, for a rebuild from Settings.
    pub origin: catalog::Origin,
    /// The Dockerfile a build runs, the app's layer included.
    pub dockerfile: String,
    /// The files it copies from the build context.
    pub files: Vec<files::File>,
}

/// Look up the image `name` built from a recipe.
pub type Find = fn(&str, &Recipe, &dyn Fn() -> bool) -> crate::Result<Option<String>>;
/// Build `recipe` as the image `name`, passing each line of the build's
/// output to the last argument; returns what new machines start from.
pub type Build = fn(&str, &Recipe, &dyn Fn() -> bool, &dyn Fn(&str)) -> crate::Result<String>;

/// How one provider holds images. Every function blocks: run them on a
/// worker.
pub struct Support {
    /// The platform new machines run, such as `linux/arm64`, which the key
    /// includes.
    pub platform: fn(&dyn Fn() -> bool) -> crate::Result<String>,
    /// The provider's name for `project`'s image with `key`.
    pub name: fn(&Project, &str) -> String,
    /// What new machines start from, when the provider holds the image `name`
    /// built from `recipe`.
    pub find: Find,
    pub build: Build,
    /// Delete what a build returned.
    pub delete: fn(&str, &dyn Fn() -> bool) -> crate::Result<()>,
    /// Whether builds can copy files from the context.
    pub context: bool,
}

/// What [`read`] needs to know about the checkout.
pub struct Checkout<'a> {
    pub host: &'a crate::teleport::Host,
    pub endpoint_id: &'a str,
    /// The checkout's root on its host.
    pub path: &'a str,
    /// The repository's name.
    pub label: &'a str,
}

/// The checkout holding the Git common directory `common_dir`, such as
/// `/src/app` for `/src/app/.git`; a bare repository has none.
pub fn checkout_of(common_dir: &str) -> Option<&str> {
    let root = common_dir.trim_end_matches('/').strip_suffix("/.git")?;
    (!root.is_empty()).then_some(root)
}

/// The sandbox image a new machine from `provider` starts from for the
/// project at `checkout`: built first when the provider does not hold it,
/// reporting its progress. `None` when the provider holds no images or the
/// project has no recipe. Blocks: run it on a worker.
pub fn for_new_machine(
    provider: crate::cloud::CloudProvider,
    checkout: &Checkout,
    cancelled: &AtomicBool,
    report: &dyn Fn(&Progress),
) -> crate::Result<Option<String>> {
    let Some(support) = crate::cloud::image_support(provider) else {
        return Ok(None);
    };
    let Some(recipe) = read(checkout, cancelled)? else {
        return Ok(None);
    };
    let stopped = || cancelled.load(std::sync::atomic::Ordering::Acquire);
    builds::ensure(provider, support, &recipe, &stopped, report).map(Some)
}

/// The recipe of the project checked out at `checkout`, or `None` when it
/// has none. Blocks: run it on a worker.
pub fn read(checkout: &Checkout, cancelled: &AtomicBool) -> Result<Option<Recipe>> {
    let Checkout {
        host,
        endpoint_id,
        path,
        label,
    } = *checkout;
    let plan = match files::read_one(host, path, recipe::HERDR, cancelled)? {
        Some(_) => recipe::herdr_plan(),
        None => {
            let mut found = None;
            for candidate in recipe::DEVCONTAINERS {
                if let Some(file) = files::read_one(host, path, candidate, cancelled)? {
                    let text = String::from_utf8_lossy(&file.bytes);
                    found = Some(recipe::devcontainer_plan(candidate, &text)?);
                    break;
                }
            }
            let Some(plan) = found else {
                return Ok(None);
            };
            plan
        }
    };
    let (dockerfile, files) = match plan {
        recipe::Plan::Image(image) => (recipe::from_image(&image), Vec::new()),
        recipe::Plan::Build {
            dockerfile,
            context,
        } => {
            let file = files::read_one(host, path, &dockerfile, cancelled)?
                .ok_or_else(|| Error::Missing(dockerfile.clone()))?;
            let text = String::from_utf8_lossy(&file.bytes).into_owned();
            let mut pathspecs = recipe::uses(&text).sources;
            if !pathspecs.is_empty() {
                pathspecs.push(".dockerignore".into());
            }
            (
                text,
                files::read(host, path, &context, &pathspecs, cancelled)?,
            )
        }
    };
    let origin = host
        .capture(
            &format!(
                "cd {} && git remote get-url origin 2>/dev/null || :\n",
                herdr_client::shell_quote(path)
            ),
            cancelled,
        )
        .map_err(Error::Host)?;
    let origin = String::from_utf8_lossy(&origin);
    Ok(Some(Recipe {
        project: Project::new(label, Some(origin.trim()), endpoint_id, path),
        origin: catalog::Origin {
            endpoint_id: endpoint_id.to_owned(),
            path: path.to_owned(),
        },
        dockerfile: recipe::compose(&dockerfile),
        files,
    }))
}

#[cfg(test)]
mod tests;
