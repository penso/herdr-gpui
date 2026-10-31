//! What a project's image is built from, read as text: its Dockerfile, the
//! files that Dockerfile copies in, and the layer this app adds on top. A
//! project names its recipe with `.herdr/sandbox/Dockerfile`, or else with a
//! dev container (`.devcontainer/devcontainer.json`), of which only the
//! Dockerfile or image it names is used: features and lifecycle scripts
//! belong to the dev container tools, not to an image build.

use super::{Error, Field, Result, Unsupported};

/// The recipe a project keeps for this app, with its directory as the
/// build context.
pub const HERDR: &str = ".herdr/sandbox/Dockerfile";
/// Where a dev container's definition can be, in the spec's order.
pub const DEVCONTAINERS: [&str; 2] = [".devcontainer/devcontainer.json", ".devcontainer.json"];
/// Where Herdr goes in the image: on the path the bridge searches, and
/// outside the home directory, which a container mounts a volume over.
const INSTALL_DIR: &str = "/usr/local/bin";

/// Where a project's Dockerfile and build context are, from its root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Build `dockerfile` with `context` as the build context.
    Build { dockerfile: String, context: String },
    /// Start from an existing image, as a dev container's `image` says.
    Image(String),
}

/// The plan for `.herdr/sandbox/Dockerfile`.
pub fn herdr_plan() -> Plan {
    Plan::Build {
        dockerfile: HERDR.to_owned(),
        context: ".herdr/sandbox".to_owned(),
    }
}

/// The plan a dev container definition at `path` (from the project root)
/// gives, with `text` its contents.
pub fn devcontainer_plan(path: &str, text: &str) -> Result<Plan> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Definition {
        image: Option<String>,
        build: Option<Build>,
        // The spec's older spellings.
        docker_file: Option<String>,
        context: Option<String>,
        docker_compose_file: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct Build {
        dockerfile: Option<String>,
        context: Option<String>,
    }
    let definition: Definition =
        serde_json::from_str(&strip_jsonc(text)).map_err(Error::Devcontainer)?;
    if definition.docker_compose_file.is_some() {
        return Err(Error::Unsupported(Unsupported::Compose));
    }
    let base = path.rsplit_once('/').map_or("", |(dir, _)| dir);
    let (dockerfile, context) = match definition.build {
        Some(build) => (build.dockerfile, build.context),
        None => (definition.docker_file, definition.context),
    };
    if let Some(dockerfile) = dockerfile {
        let context = context.unwrap_or_else(|| ".".to_owned());
        return Ok(Plan::Build {
            dockerfile: join(base, &dockerfile)?,
            context: join(base, &context)?,
        });
    }
    match definition.image {
        Some(image) if crate::cloud::names::image_reference(&image) => Ok(Plan::Image(image)),
        Some(_) => Err(Error::Field(Field::DevcontainerImage)),
        None => Err(Error::Unsupported(Unsupported::NoBuild)),
    }
}

/// `relative` resolved against `base`, both from the project root, refused
/// when it leaves the project.
pub fn join(base: &str, relative: &str) -> Result<String> {
    if relative.starts_with('/') || relative.contains('\0') {
        return Err(Error::Field(Field::ProjectPath));
    }
    let mut parts: Vec<&str> = base.split('/').filter(|part| !part.is_empty()).collect();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or(Error::Field(Field::ProjectPath))?;
            }
            part => parts.push(part),
        }
    }
    Ok(if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    })
}

/// JSON with comments and trailing commas, as dev container files are
/// written, made plain JSON. Strings are left alone.
pub fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            match c {
                '\\' => out.extend(chars.next()),
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match (c, chars.peek()) {
            ('"', _) => {
                in_string = true;
                out.push(c);
            }
            ('/', Some('/')) => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            ('/', Some('*')) => {
                chars.next();
                let mut last = '\0';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            _ => out.push(c),
        }
    }
    remove_trailing_commas(&out)
}

fn remove_trailing_commas(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for (index, c) in text.char_indices() {
        if in_string {
            in_string = c != '"' || escaped;
            escaped = c == '\\' && !escaped;
            out.push(c);
            continue;
        }
        if c == '"' {
            in_string = true;
        }
        if c == ',' {
            let next = text[index + 1..].trim_start().chars().next();
            if matches!(next, Some('}' | ']')) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// What reading a Dockerfile tells the build: which files from the context
/// it copies in, and which user the final stage ends as.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Uses {
    /// Paths and patterns from the context, as `COPY` and `ADD` name them.
    /// `.` when a source cannot be told, such as one built from a variable.
    pub sources: Vec<String>,
    /// The last stage's `USER`, which the added layer restores.
    pub user: Option<String>,
}

/// Read `dockerfile` for the files it uses from its context. Sources from
/// another stage or image (`--from`), URLs, and heredocs use none.
pub fn uses(dockerfile: &str) -> Uses {
    let mut uses = Uses::default();
    for instruction in instructions(dockerfile) {
        let (word, rest) = instruction
            .split_once(char::is_whitespace)
            .unwrap_or((instruction.as_str(), ""));
        match word.to_ascii_uppercase().as_str() {
            "FROM" => uses.user = None,
            "USER" => uses.user = Some(rest.trim().to_owned()),
            "COPY" | "ADD" => {
                let Some(sources) = copied(rest) else {
                    continue;
                };
                for source in sources {
                    if !uses.sources.contains(&source) {
                        uses.sources.push(source);
                    }
                }
            }
            _ => {}
        }
    }
    uses
}

/// The sources of one `COPY` or `ADD`, or `None` when they come from
/// elsewhere than the context.
fn copied(arguments: &str) -> Option<Vec<String>> {
    let mut words: Vec<String> = Vec::new();
    let trimmed = arguments.trim();
    let mut rest = trimmed;
    // Flags come first: `--chown=…`, `--from=…`, `--link`.
    while let Some(after) = rest.strip_prefix("--") {
        let (flag, tail) = after.split_once(char::is_whitespace).unwrap_or((after, ""));
        if flag.starts_with("from=") {
            return None;
        }
        rest = tail.trim_start();
    }
    if rest.starts_with('[') {
        words = serde_json::from_str::<Vec<String>>(rest).ok()?;
    } else {
        words.extend(rest.split_whitespace().map(str::to_owned));
    }
    // The last word is the destination.
    words.pop()?;
    Some(
        words
            .into_iter()
            .filter(|source| !source.starts_with("<<") && !source.contains("://"))
            .filter(|source| !source.starts_with("git@"))
            .map(|source| {
                if source.contains('$') {
                    ".".to_owned()
                } else {
                    let source = source.trim_start_matches("./").trim_start_matches('/');
                    if source.is_empty() {
                        ".".to_owned()
                    } else {
                        source.to_owned()
                    }
                }
            })
            .collect(),
    )
}

/// The Dockerfile's instructions with continuations joined, comments and
/// heredoc bodies left out.
fn instructions(dockerfile: &str) -> Vec<String> {
    let mut instructions = Vec::new();
    let mut current = String::new();
    let mut heredoc: Option<(String, bool)> = None;
    for line in dockerfile.lines() {
        if let Some((end, strip_tabs)) = &heredoc {
            let line = if *strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };
            if line == end {
                heredoc = None;
            }
            continue;
        }
        let trimmed = line.trim();
        if current.is_empty() && (trimmed.is_empty() || trimmed.starts_with('#')) {
            continue;
        }
        if let Some(body) = trimmed.strip_suffix('\\') {
            current.push_str(body);
            current.push(' ');
            continue;
        }
        current.push_str(trimmed);
        heredoc = heredoc_end(&current);
        instructions.push(std::mem::take(&mut current));
    }
    if !current.is_empty() {
        instructions.push(current);
    }
    instructions
}

/// The word ending a heredoc this instruction opens, and whether its body's
/// leading tabs are stripped (`<<-`).
fn heredoc_end(instruction: &str) -> Option<(String, bool)> {
    let start = instruction.find("<<")?;
    let rest = &instruction[start + 2..];
    let (rest, strip_tabs) = match rest.strip_prefix('-') {
        Some(rest) => (rest, true),
        None => (rest, false),
    };
    let word: String = rest
        .trim_start_matches(['"', '\''])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    (!word.is_empty()).then_some((word, strip_tabs))
}

/// The layer this app adds to every image: Herdr, installed where the bridge
/// finds it, so a new machine skips the installer. It runs as root, then
/// gives the image back the user its own last stage chose.
pub fn layer(user: Option<&str>) -> String {
    let mut layer = format!(
        "\n# Added by Herdr UI: install Herdr so new sandboxes start ready.\n\
         USER root\n\
         RUN command -v curl >/dev/null 2>&1 || {{ echo 'Herdr UI needs curl in the image to install Herdr' >&2; exit 4; }}; \\\n    \
         curl -fsSL https://herdr.dev/install.sh | HERDR_INSTALL_DIR={INSTALL_DIR} sh\n"
    );
    if let Some(user) = user {
        layer.push_str(&format!("USER {user}\n"));
    }
    layer
}

/// The Dockerfile a build runs: the project's own, then [`layer`].
pub fn compose(dockerfile: &str) -> String {
    let user = uses(dockerfile).user;
    let mut composed = dockerfile.trim_end().to_owned();
    composed.push('\n');
    composed.push_str(&layer(user.as_deref()));
    composed
}

/// The Dockerfile for a plan that starts from an existing image.
pub fn from_image(image: &str) -> String {
    format!("FROM {image}\n")
}

#[cfg(test)]
mod tests;
