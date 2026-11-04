//! Sandbox images on this computer's engine: built with `docker build` or
//! `podman build` from the recipe's files, written to a private temporary
//! directory, and tagged `herdr-<project>:<key>` with a label naming the
//! project, so the app finds and deletes only its own.

use super::Engine;
use crate::cloud::image::{Project, Recipe, Support, key};
use std::{fs, io::Write, time::Duration};

/// The label every image this app builds carries, holding its project.
const LABEL: &str = "dev.herdr-gpui.image";
const QUICK: Duration = Duration::from_secs(30);
/// A build's longest run: images with toolchains take a while.
const BUILD: Duration = Duration::from_secs(60 * 60);

pub const SUPPORT: Support = Support {
    platform,
    name,
    find,
    build,
    delete,
    context: true,
};

/// The engine new containers use, as Settings chose it.
fn engine() -> crate::Result<Engine> {
    // Live tests choose their engine without touching the user's config.
    #[cfg(test)]
    if let Some(engine) = std::env::var("HERDR_TEST_CONTAINER_ENGINE")
        .ok()
        .and_then(|key| Engine::from_key(&key))
    {
        return Ok(engine);
    }
    let settings = crate::config::load()?.container.settings()?;
    Ok(Engine::pick(settings.engine)?)
}

fn platform(cancelled: &dyn Fn() -> bool) -> crate::Result<String> {
    let engine = engine()?;
    let format = match engine {
        Engine::Docker => "{{.OSType}}/{{.Architecture}}",
        Engine::Podman => "{{.Host.OS}}/{{.Host.Arch}}",
    };
    let platform = engine.run("info", ["info", "--format", format], QUICK, cancelled)?;
    Ok(format!("{} {platform}", engine.key()))
}

fn name(project: &Project, key: &str) -> String {
    format!("herdr-{}:{}", project.slug(), key::short(key))
}

fn find(name: &str, _: &Recipe, cancelled: &dyn Fn() -> bool) -> crate::Result<Option<String>> {
    let engine = engine()?;
    let (code, ..) = engine.output(
        "image inspect",
        ["image", "inspect", name],
        QUICK,
        cancelled,
    )?;
    Ok((code == Some(0)).then(|| name.to_owned()))
}

fn build(
    name: &str,
    recipe: &Recipe,
    cancelled: &dyn Fn() -> bool,
    line: &dyn Fn(&str),
) -> crate::Result<String> {
    let engine = engine()?;
    let directory = tempfile::tempdir().map_err(super::Error::BuildContext)?;
    let context = directory.path().join("context");
    fs::create_dir(&context).map_err(super::Error::BuildContext)?;
    for file in &recipe.files {
        let path = context.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(super::Error::BuildContext)?;
        }
        let mut written = fs::File::create(&path).map_err(super::Error::BuildContext)?;
        written
            .write_all(&file.bytes)
            .map_err(super::Error::BuildContext)?;
        #[cfg(unix)]
        if file.executable {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .map_err(super::Error::BuildContext)?;
        }
    }
    let dockerfile = directory.path().join("Dockerfile");
    fs::write(&dockerfile, &recipe.dockerfile).map_err(super::Error::BuildContext)?;
    let label = format!("{LABEL}={}", recipe.project.id);
    let mut args = vec!["build".to_owned()];
    // BuildKit numbers its steps only in plain progress.
    if engine == Engine::Docker {
        args.push("--progress=plain".into());
    }
    args.extend(["--label".into(), label, "--tag".into(), name.to_owned()]);
    args.push("--file".into());
    args.push(dockerfile.to_string_lossy().into_owned());
    args.push(context.to_string_lossy().into_owned());
    engine.stream("build", &args, BUILD, cancelled, line)?;
    Ok(name.to_owned())
}

fn delete(reference: &str, cancelled: &dyn Fn() -> bool) -> crate::Result<()> {
    let engine = engine()?;
    // Never an image this app did not label: the user's own keep their tags.
    let format = format!("{{{{index .Config.Labels \"{LABEL}\"}}}}");
    let (code, stdout, _) = engine.output(
        "image inspect",
        ["image", "inspect", "--format", &format, reference],
        QUICK,
        cancelled,
    )?;
    if code != Some(0) {
        return Ok(());
    }
    let owner = String::from_utf8_lossy(&stdout);
    let owner = owner.trim();
    if owner.is_empty() || owner == "<no value>" {
        return Err(super::Error::ImageNotOurs(reference.to_owned()).into());
    }
    let (code, _, stderr) =
        engine.output("image rm", ["image", "rm", reference], QUICK, cancelled)?;
    if code == Some(0) || in_use(&String::from_utf8_lossy(&stderr)) {
        return Ok(());
    }
    Err(super::Error::Failed {
        engine: engine.key(),
        action: "image rm",
        detail: super::engine::detail(&stderr),
    }
    .into())
}

/// Whether `image rm` refused because a container uses the image. The image
/// then stays for those containers: forcing it would make Podman delete them.
fn in_use(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    // Docker: "conflict: … container … is using its referenced image";
    // Podman: "image is in use by a container".
    stderr.contains("conflict") || stderr.contains("in use") || stderr.contains("is using")
}

#[cfg(test)]
mod tests;
