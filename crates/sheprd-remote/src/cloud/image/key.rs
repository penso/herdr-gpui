//! Which project an image belongs to, and the key that says whether a built
//! image still matches its recipe. Clones and worktrees of one repository
//! share their images: a project is its `origin` remote when it has one. The
//! key is a SHA-256 of everything the build reads, so an unchanged recipe
//! reuses its image and any change builds a new one.

use super::files::File;
use sha2::{Digest, Sha256};

/// Bumped whenever how an image is built changes, so older images are not
/// mistaken for current ones.
const FORMAT: &str = "herdr-gpui sandbox image 1";
/// Hex digits of a hash a name carries: enough to tell images apart.
const SHORT: usize = 12;
/// The longest project name a resource name keeps.
const SLUG_LIMIT: usize = 24;

/// A project, as images are filed by.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Project {
    /// Stable across clones: a hash of the `origin` remote, or of the host
    /// and path for a repository without one.
    pub id: String,
    /// The repository's name, as the UI shows it.
    pub label: String,
}

impl Project {
    /// The project `label` whose `origin` remote is `origin`, or which is at
    /// `path` on the endpoint `endpoint_id` when it has none.
    pub fn new(label: &str, origin: Option<&str>, endpoint_id: &str, path: &str) -> Self {
        let identity = match origin.and_then(crate::teleport::remote::RemoteId::parse) {
            Some(remote) => format!("remote {}", remote.as_str()),
            None => format!("path {endpoint_id} {path}"),
        };
        Self {
            id: hex(&Sha256::digest(identity.as_bytes()))[..16].to_owned(),
            label: label.to_owned(),
        }
    }

    /// The lowercase name resources for this project start with, such as
    /// `herdr-gpui-3fa2c1d0`: its label, then a hash that keeps two projects
    /// of one name apart.
    pub fn slug(&self) -> String {
        let mut slug = String::new();
        for c in self.label.chars() {
            match c {
                'a'..='z' | '0'..='9' => slug.push(c),
                'A'..='Z' => slug.push(c.to_ascii_lowercase()),
                _ if !slug.is_empty() && !slug.ends_with('-') => slug.push('-'),
                _ => {}
            }
            if slug.len() >= SLUG_LIMIT {
                break;
            }
        }
        let slug = slug.trim_end_matches('-');
        let slug = if slug.is_empty() { "project" } else { slug };
        format!("{slug}-{}", &self.id[..8])
    }
}

/// The key of a build: the composed Dockerfile, every context file, and the
/// platform it is built for.
pub fn key(dockerfile: &str, files: &[File], platform: &str) -> String {
    let mut hash = Sha256::new();
    for part in [FORMAT, platform, dockerfile] {
        hash.update(part.len().to_le_bytes());
        hash.update(part.as_bytes());
    }
    for file in files {
        hash.update(file.path.len().to_le_bytes());
        hash.update(file.path.as_bytes());
        hash.update([u8::from(file.executable)]);
        hash.update(file.bytes.len().to_le_bytes());
        hash.update(&file.bytes);
    }
    hex(&hash.finalize())
}

/// The part of `key` a resource name carries.
pub fn short(key: &str) -> &str {
    &key[..SHORT.min(key.len())]
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(test)]
mod tests;
