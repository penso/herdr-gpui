//! The Sprites REST calls this app makes: create, read, list, and delete a
//! sprite, and register Herdr's server as a service so a sprite that slept
//! has it running again when it wakes.

use super::{Error, Result, Status, http::Request, settings::sprite_name};
use crate::cloud::Value;
use serde::{Deserialize, Serialize};

/// The service that keeps Herdr's server running across sleeps.
const SERVICE: &str = "herdr";
/// Runs the server where Herdr's installer puts it, replacing the shell.
const SERVER: &str = r#"PATH="$HOME/.local/bin:$PATH"; exec herdr server"#;

/// A sprite as the API describes it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Sprite {
    pub name: String,
    /// The runtime status, such as `running`, `warm`, or `cold`.
    #[serde(default)]
    pub status: String,
}

#[derive(Deserialize)]
struct List {
    #[serde(default)]
    sprites: Vec<Sprite>,
}

fn path(name: &str) -> Result<String> {
    if !sprite_name(name) {
        return Err(Error::Field(Value::SpriteName));
    }
    Ok(format!("/v1/sprites/{name}"))
}

impl Request<'_> {
    pub fn create(&self, name: &str) -> Result<Sprite> {
        #[derive(Serialize)]
        struct Create<'a> {
            name: &'a str,
        }
        path(name)?;
        self.post("create sprite", "/v1/sprites", &Create { name })
    }

    /// The sprite `name`, or [`Error::Gone`] when it no longer exists.
    pub fn sprite(&self, name: &str) -> Result<Sprite> {
        match self.get("read sprite", &path(name)?) {
            Err(Error::Status(Status { code: 404, .. })) => Err(Error::Gone(name.to_owned())),
            other => other,
        }
    }

    /// The sprites whose names start with `prefix`, at most a page of them.
    pub fn sprites(&self, prefix: &str) -> Result<Vec<Sprite>> {
        if !prefix.is_empty() && !sprite_name(prefix) {
            return Err(Error::Field(Value::SpritePrefix));
        }
        let list: List = self.get(
            "list sprites",
            &format!("/v1/sprites?prefix={prefix}&max_results=500"),
        )?;
        Ok(list.sprites)
    }

    pub fn delete_sprite(&self, name: &str) -> Result<()> {
        self.delete("delete sprite", &path(name)?)
    }

    /// Run Herdr's server as the sprite's service, started now and on every
    /// wake.
    pub fn serve_herdr(&self, name: &str) -> Result<()> {
        #[derive(Serialize)]
        struct Service<'a> {
            cmd: &'a str,
            args: [&'a str; 2],
        }
        self.put(
            "register Herdr's server",
            &format!("{}/services/{SERVICE}", path(name)?),
            &Service {
                cmd: "/bin/sh",
                args: ["-c", SERVER],
            },
        )
    }
}
