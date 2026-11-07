//! One container's lifecycle on the engine. A container is named by its
//! device ID, keeps its home directory in a named volume beside it, and is
//! labelled so containers this app made can be told from the user's own.
//!
//! Hardening is the engine's default seccomp and capability set plus
//! `no-new-privileges`, an init process, and CPU, memory, and process limits;
//! the container never gets the engine's socket, host networking, or a mount
//! of this computer's files. Agents work as an unprivileged `herdr` user.

use super::{Engine, Error, Result, Settings, engine::detail};
use std::{process::Command, time::Duration};

/// The label every container and volume this app makes carries, with the
/// device ID as its value.
pub const LABEL: &str = "dev.herdr-gpui.device";
/// The user agents run as, and its home, where the volume is mounted.
pub const USER: &str = "herdr";
pub const HOME: &str = "/home/herdr";

const QUICK: Duration = Duration::from_secs(30);
const PULL: Duration = Duration::from_secs(15 * 60);

/// Make the `herdr` user own its home, whatever the image ships: Debian and
/// Ubuntu have `useradd`, Alpine and BusyBox images `adduser`. Run again on
/// every start, so it must stay idempotent.
const PREPARE: &str = r#"set -e
if ! id herdr >/dev/null 2>&1; then
  if command -v useradd >/dev/null 2>&1; then
    useradd --home-dir /home/herdr --no-create-home --shell /bin/sh --user-group herdr
  else
    adduser -D -H -h /home/herdr -s /bin/sh herdr
  fi
fi
chown herdr /home/herdr"#;

/// What the engine says about a container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Running,
    /// Created, exited, or paused: starting it brings it back with its files.
    Stopped,
    Missing,
}

pub fn volume(id: &str) -> String {
    format!("{id}-home")
}

/// The image reference pinned to the digest it resolved to, so a container
/// made later from the same settings, or recreated, gets the same image.
fn pinned(engine: Engine, image: &str, cancelled: &dyn Fn() -> bool) -> Result<String> {
    let (code, ..) = engine.output(
        "image inspect",
        ["image", "inspect", image],
        QUICK,
        cancelled,
    )?;
    if code != Some(0) {
        engine.run("pull", ["pull", image], PULL, cancelled)?;
    }
    let digests = engine.run(
        "image inspect",
        [
            "image",
            "inspect",
            "--format",
            "{{json .RepoDigests}}",
            image,
        ],
        QUICK,
        cancelled,
    )?;
    Ok(serde_json::from_str::<Vec<String>>(&digests)
        .ok()
        .and_then(|digests| digests.into_iter().next())
        .filter(|digest| crate::cloud::names::image_reference(digest))
        // A locally built image has no registry digest; its tag is all there is.
        .unwrap_or_else(|| image.to_owned()))
}

/// The arguments of `run` for the device `id`.
pub fn run_args(id: &str, image: &str, settings: &Settings) -> Vec<String> {
    let label = format!("{LABEL}={id}");
    [
        "run",
        "--detach",
        "--name",
        id,
        "--hostname",
        id,
        "--label",
        &label,
        "--init",
        "--security-opt",
        "no-new-privileges",
        "--cpus",
        &settings.cpus,
        "--memory",
        &settings.memory,
        "--pids-limit",
        &settings.pids.to_string(),
        "--volume",
        &format!("{}:{HOME}", volume(id)),
        "--env",
        &format!("HOME={HOME}"),
        // An image's own `ENTRYPOINT` would take `sleep infinity` as its
        // arguments and could exit, so the keep-alive replaces it.
        "--entrypoint",
        "sleep",
        image,
        "infinity",
    ]
    .map(str::to_owned)
    .into()
}

/// Make the container `id` and its home volume, and prepare its user. A
/// failure removes whatever was made, so nothing is left running unlisted.
pub fn create(
    engine: Engine,
    id: &str,
    settings: &Settings,
    cancelled: &dyn Fn() -> bool,
    pulling: &dyn Fn(),
) -> Result<()> {
    engine.run(
        "version",
        ["version", "--format", "{{.Server.Version}}"],
        QUICK,
        cancelled,
    )?;
    pulling();
    let image = pinned(engine, &settings.image, cancelled)?;
    // `volume create` reuses a volume of the same name, and a failed `run`
    // would then delete whatever else holds the name: refuse a name in use.
    for kind in [Kind::Container, Kind::Volume] {
        if owner(engine, kind, id, cancelled)? != Owner::Missing {
            return Err(Error::NameTaken(id.to_owned()));
        }
    }
    tracing::info!(category = "container", id, image, "Creating container");
    let label = format!("{LABEL}={id}");
    let made = engine
        .run(
            "volume create",
            ["volume", "create", "--label", &label, &volume(id)],
            QUICK,
            cancelled,
        )
        .and_then(|_| engine.run("run", run_args(id, &image, settings), QUICK, cancelled))
        .and_then(|_| prepare(engine, id, cancelled));
    if let Err(error) = made {
        // Never cancelled: a half-made container must not outlive the job.
        if let Err(cleanup) = delete(engine, id, &|| false) {
            tracing::warn!(category = "container", id, error = %cleanup, "Could not remove a failed container");
        }
        return Err(error);
    }
    Ok(())
}

fn prepare(engine: Engine, id: &str, cancelled: &dyn Fn() -> bool) -> Result<()> {
    engine
        .run(
            "exec",
            ["exec", "--user", "root", id, "/bin/sh", "-c", PREPARE],
            QUICK,
            cancelled,
        )
        .map(drop)
}

pub fn state(engine: Engine, id: &str, cancelled: &dyn Fn() -> bool) -> Result<State> {
    let (code, stdout, stderr) = engine.output(
        "inspect",
        ["container", "inspect", "--format", "{{.State.Status}}", id],
        QUICK,
        cancelled,
    )?;
    if code != Some(0) {
        let detail = detail(&stderr);
        // Docker says "No such container", Podman "no such container".
        if detail.to_ascii_lowercase().contains("no such") {
            return Ok(State::Missing);
        }
        return Err(Error::Failed {
            engine: engine.key(),
            action: "inspect",
            detail,
        });
    }
    Ok(match String::from_utf8_lossy(&stdout).trim() {
        "running" => State::Running,
        _ => State::Stopped,
    })
}

/// Start the container `id` if it is not running.
pub fn ensure_running(engine: Engine, id: &str, cancelled: &dyn Fn() -> bool) -> Result<()> {
    match state(engine, id, cancelled)? {
        State::Running => Ok(()),
        State::Missing => Err(Error::Gone(id.to_owned())),
        State::Stopped => {
            // A paused container refuses `start`; `unpause` is harmless otherwise.
            let _ = engine.output("unpause", ["unpause", id], QUICK, cancelled);
            engine.run("start", ["start", id], QUICK, cancelled)?;
            // Podman gives the volume's mount point back to root on each
            // start, which would leave the agents' user without its home.
            prepare(engine, id, cancelled)
        }
    }
}

/// Stop the container `id`, keeping it and its files.
pub fn stop(engine: Engine, id: &str, cancelled: &dyn Fn() -> bool) -> Result<()> {
    engine
        .run("stop", ["stop", id], Duration::from_secs(60), cancelled)
        .map(drop)
}

/// A container or a volume, for [`owner`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Container,
    Volume,
}

/// Who holds a name on the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Missing,
    /// Labelled as the device of the same ID: this app made it for that device.
    Ours,
    /// Anything else, such as the user's own container of that name.
    Other,
}

/// Who holds the container `id`, or its home volume.
fn owner(engine: Engine, kind: Kind, id: &str, cancelled: &dyn Fn() -> bool) -> Result<Owner> {
    let (name, format) = match kind {
        Kind::Container => (
            id.to_owned(),
            format!("{{{{index .Config.Labels \"{LABEL}\"}}}}"),
        ),
        Kind::Volume => (volume(id), format!("{{{{index .Labels \"{LABEL}\"}}}}")),
    };
    let noun = match kind {
        Kind::Container => "container",
        Kind::Volume => "volume",
    };
    let (code, stdout, stderr) = engine.output(
        "inspect",
        [noun, "inspect", "--format", &format, &name],
        QUICK,
        cancelled,
    )?;
    if code != Some(0) {
        let detail = detail(&stderr);
        // Docker says "No such container/volume", Podman "no such ...".
        if detail.to_ascii_lowercase().contains("no such") {
            return Ok(Owner::Missing);
        }
        return Err(Error::Failed {
            engine: engine.key(),
            action: "inspect",
            detail,
        });
    }
    Ok(if String::from_utf8_lossy(&stdout).trim() == id {
        Owner::Ours
    } else {
        Owner::Other
    })
}

/// The containers on the engine this app made, by device ID: each labelled
/// with its own name. At most a page of them, which is far past the cap.
pub fn labelled(engine: Engine, cancelled: &dyn Fn() -> bool) -> Result<Vec<String>> {
    const PAGE: usize = 128;
    let names = engine.run(
        "ps",
        [
            "ps",
            "--all",
            "--filter",
            &format!("label={LABEL}"),
            "--format",
            "{{.Names}}",
        ],
        QUICK,
        cancelled,
    )?;
    let mut ours = Vec::new();
    for name in names.lines().map(str::trim).take(PAGE) {
        if crate::cloud::names::image_reference(name)
            && owner(engine, Kind::Container, name, cancelled)? == Owner::Ours
        {
            ours.push(name.to_owned());
        }
    }
    Ok(ours)
}

/// Remove the container `id` and its home volume, when they are this app's
/// for that device. Either one already gone is not an error; one of the same
/// name that this app did not make is refused and left alone.
pub fn delete(engine: Engine, id: &str, cancelled: &dyn Fn() -> bool) -> Result<()> {
    for kind in [Kind::Container, Kind::Volume] {
        match owner(engine, kind, id, cancelled)? {
            Owner::Missing => continue,
            Owner::Other => return Err(Error::NotOurs(id.to_owned())),
            Owner::Ours => {}
        }
        let (action, args) = match kind {
            Kind::Container => (
                "rm",
                vec!["rm".to_owned(), "--force".to_owned(), id.to_owned()],
            ),
            Kind::Volume => (
                "volume rm",
                vec!["volume".to_owned(), "rm".to_owned(), volume(id)],
            ),
        };
        let (code, _, stderr) = engine.output(action, args, QUICK, cancelled)?;
        let detail = detail(&stderr);
        if code != Some(0) && !detail.to_ascii_lowercase().contains("no such") {
            return Err(Error::Failed {
                engine: engine.key(),
                action,
                detail,
            });
        }
    }
    Ok(())
}

/// Start Herdr's headless server in the container `id` and wait until it
/// answers, so `herdr` commands work before any window has connected; a
/// bridge would start it too, but only once a window attaches.
pub fn start_server(engine: Engine, id: &str, cancelled: &dyn Fn() -> bool) -> Result<()> {
    let env = [format!("HOME={HOME}"), format!("USER={USER}")];
    let mut detached = vec!["exec", "--detach", "--user", USER, "--workdir", HOME];
    for pair in &env {
        detached.extend(["--env", pair]);
    }
    detached.extend([id, "/bin/sh", "-c", SERVER]);
    engine.run("exec", detached, QUICK, cancelled)?;
    let deadline = std::time::Instant::now() + QUICK;
    loop {
        let mut status = vec!["exec", "--user", USER];
        for pair in &env {
            status.extend(["--env", pair]);
        }
        status.extend([id, "/bin/sh", "-c", STATUS]);
        let (code, ..) = engine.output("exec", status, QUICK, cancelled)?;
        if code == Some(0) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(Error::Timeout {
                engine: engine.key(),
                action: "starting Herdr's server",
            });
        }
        if cancelled() {
            return Err(Error::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Run Herdr's server where the installer put it, replacing the shell.
const SERVER: &str = r#"PATH="$HOME/.local/bin:$PATH"; exec herdr server >/dev/null 2>&1"#;
/// Succeed once that server answers.
const STATUS: &str = r#"PATH="$HOME/.local/bin:$PATH"; herdr status server 2>/dev/null | grep -q '^status: running'"#;

/// A command that runs, as the `herdr` user in its home, the one shell string
/// appended to it, as `ssh` runs its remote command: the bridge, Herdr's
/// installer, and Teleport's scripts all append theirs.
pub fn exec(engine: Engine, id: &str) -> Result<Command> {
    engine.command([
        "exec",
        "--interactive",
        "--user",
        USER,
        "--workdir",
        HOME,
        "--env",
        &format!("HOME={HOME}"),
        "--env",
        &format!("USER={USER}"),
        id,
        "/bin/sh",
        "-c",
    ])
}
