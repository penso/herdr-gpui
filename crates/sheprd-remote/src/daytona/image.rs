//! Sandbox images as Daytona snapshots, which Daytona builds from the
//! recipe's Dockerfile on its own runners and keeps ready, so a sandbox
//! made from one starts at once. Files a Dockerfile copies would have to be
//! uploaded to Daytona's object storage first, which this app does not do
//! yet, so recipes with `COPY` or `ADD` from their context are refused
//! before anything is sent.
//!
//! Snapshot names are predictable and shared by everyone in the Daytona
//! organization, so a snapshot is only used when Daytona built it from this
//! recipe's exact Dockerfile; one of the same name made any other way is
//! replaced rather than trusted.

use super::{Error, Result, Settings, http::Request, store};
use crate::cloud::Value;
use crate::cloud::image::{Project, Recipe, Support, key};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_secs(3);
/// A build's longest run: images with toolchains take a while.
const BUILD: Duration = Duration::from_secs(60 * 60);

pub const SUPPORT: Support = Support {
    platform,
    name,
    find,
    build,
    delete,
    context: false,
};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    #[serde(default)]
    state: String,
    #[serde(default)]
    error_reason: Option<String>,
    #[serde(default)]
    build_info: Option<BuildInfo>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildInfo {
    #[serde(default)]
    dockerfile_content: String,
    #[serde(default)]
    context_hashes: Vec<String>,
}

/// What a snapshot's state means for a build.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Readiness {
    Ready,
    Failed(String),
    /// Daytona's word for where it is, such as `building`.
    Pending(String),
}

impl Snapshot {
    /// Whether Daytona built this snapshot from `recipe`'s Dockerfile and no
    /// build context, as this app does, so nothing else went into it.
    fn built_from(&self, recipe: &Recipe) -> bool {
        self.build_info.as_ref().is_some_and(|info| {
            info.dockerfile_content == recipe.dockerfile && info.context_hashes.is_empty()
        })
    }

    fn readiness(&self) -> Readiness {
        match self.state.as_str() {
            "active" => Readiness::Ready,
            "error" | "build_failed" => Readiness::Failed(
                self.error_reason
                    .clone()
                    .filter(|reason| !reason.is_empty())
                    .unwrap_or_else(|| self.state.replace('_', " ")),
            ),
            other => Readiness::Pending(other.replace('_', " ")),
        }
    }
}

fn settings() -> crate::Result<Settings> {
    Ok(crate::config::load()?
        .daytona
        .settings()?
        .ok_or(Error::Account)?)
}

/// Snapshot names are this app's own, but still go into paths.
fn segment(name: &str) -> Result<&str> {
    let valid = !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    valid
        .then_some(name)
        .ok_or(Error::Field(Value::DaytonaSnapshot))
}

/// The snapshot `name`, or `None` when Daytona has none of that name.
fn snapshot(request: &Request, name: &str) -> Result<Option<Snapshot>> {
    match request.get("snapshot", &format!("/snapshots/{}", segment(name)?)) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(Error::Status(super::Status { code: 404, .. })) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Daytona builds on its own x86-64 runners.
fn platform(_: &dyn Fn() -> bool) -> crate::Result<String> {
    Ok("daytona linux/amd64".into())
}

fn name(project: &Project, key: &str) -> String {
    format!("herdr-{}-{}", project.slug(), key::short(key))
}

fn find(name: &str, recipe: &Recipe, _: &dyn Fn() -> bool) -> crate::Result<Option<String>> {
    let settings = settings()?;
    let key = store::key(&settings)?;
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let ready = snapshot(&request, name)?.is_some_and(|snapshot| {
        snapshot.built_from(recipe) && snapshot.readiness() == Readiness::Ready
    });
    Ok(ready.then(|| name.to_owned()))
}

fn build(
    name: &str,
    recipe: &Recipe,
    cancelled: &dyn Fn() -> bool,
    line: &dyn Fn(&str),
) -> crate::Result<String> {
    let settings = settings()?;
    let key = store::key(&settings)?;
    let request = Request {
        settings: &settings,
        key: &key,
    };
    let found = snapshot(&request, name)?
        .map(|snapshot| (snapshot.built_from(recipe), snapshot.readiness()));
    match found {
        Some((true, Readiness::Ready)) => return Ok(name.to_owned()),
        Some((true, Readiness::Pending(_))) => {}
        // A failed build of the same recipe is tried again, and a snapshot
        // of this name built from anything else is not trusted.
        Some((true, Readiness::Failed(_)) | (false, _)) => {
            request.delete("delete_snapshot", &format!("/snapshots/{}", segment(name)?))?;
            create(&request, name, recipe)?;
        }
        None => create(&request, name, recipe)?,
    }
    line("Daytona is building the snapshot");
    let deadline = Instant::now() + BUILD;
    let mut last = String::new();
    loop {
        let readiness = snapshot(&request, name)?
            .ok_or_else(|| Error::Snapshot("it was deleted while building".into()))?
            .readiness();
        match readiness {
            Readiness::Ready => return Ok(name.to_owned()),
            Readiness::Failed(reason) => return Err(Error::Snapshot(reason).into()),
            Readiness::Pending(state) if state != last => {
                line(&format!("Daytona: {state}"));
                last = state;
            }
            Readiness::Pending(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(Error::Snapshot("it did not finish within an hour".into()).into());
        }
        let wake = Instant::now() + POLL;
        while Instant::now() < wake {
            if cancelled() {
                return Err(Error::Cancelled.into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

fn create(request: &Request, name: &str, recipe: &Recipe) -> Result<()> {
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Create<'a> {
        name: &'a str,
        build_info: BuildInfo<'a>,
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct BuildInfo<'a> {
        dockerfile_content: &'a str,
        context_hashes: [&'a str; 0],
    }
    let _: serde_json::Value = request.post(
        "create_snapshot",
        "/snapshots",
        &Create {
            name: segment(name)?,
            build_info: BuildInfo {
                dockerfile_content: &recipe.dockerfile,
                context_hashes: [],
            },
        },
    )?;
    Ok(())
}

fn delete(reference: &str, _: &dyn Fn() -> bool) -> crate::Result<()> {
    let settings = settings()?;
    let key = store::key(&settings)?;
    let request = Request {
        settings: &settings,
        key: &key,
    };
    match request.delete(
        "delete_snapshot",
        &format!("/snapshots/{}", segment(reference)?),
    ) {
        Err(Error::Status(super::Status { code: 404, .. })) | Ok(()) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests;
