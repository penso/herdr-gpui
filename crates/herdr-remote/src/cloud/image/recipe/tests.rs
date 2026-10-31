#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn copy_and_add_name_their_context_sources() {
    let dockerfile = r#"
# syntax=docker/dockerfile:1
FROM rust:1.90 AS build
COPY --chown=me:me Cargo.toml Cargo.lock ./
COPY ["src", "/app/src"]
ADD ./vendor/ /vendor/
ADD https://example.com/tool.tgz /opt/
COPY --from=build /out /out
COPY --link \
    scripts/setup.sh /usr/local/bin/
RUN <<EOF
COPY not-an-instruction /x
EOF
COPY <<EOF /etc/motd
hello
EOF
COPY $DIR/config /etc/
"#;
    let uses = uses(dockerfile);
    assert_eq!(
        uses.sources,
        [
            "Cargo.toml",
            "Cargo.lock",
            "src",
            "vendor/",
            "scripts/setup.sh",
            "."
        ]
    );
}

#[test]
fn the_last_stages_user_is_restored_after_the_layer() {
    let dockerfile = "FROM a AS one\nUSER builder\nFROM b\nUSER dev\nRUN true\n";
    assert_eq!(uses(dockerfile).user.as_deref(), Some("dev"));
    let composed = compose(dockerfile);
    assert!(composed.starts_with(dockerfile));
    assert!(composed.contains("HERDR_INSTALL_DIR=/usr/local/bin"));
    assert!(composed.trim_end().ends_with("USER dev"));
    // A new stage resets the user it inherits.
    assert_eq!(uses("FROM a\nUSER dev\nFROM b\n").user, None);
    assert!(compose("FROM b\n").trim_end().ends_with("sh"));
}

#[test]
fn a_dev_container_names_its_dockerfile_relative_to_itself() {
    let text = r#"{
        // A comment, and a trailing comma.
        "name": "app /* not a comment */",
        "build": { "dockerfile": "Dockerfile", "context": "..", },
        "features": { "ghcr.io/devcontainers/features/node:1": {} },
    }"#;
    assert_eq!(
        devcontainer_plan(".devcontainer/devcontainer.json", text).unwrap(),
        Plan::Build {
            dockerfile: ".devcontainer/Dockerfile".into(),
            context: ".".into(),
        }
    );
    // The spec's older spelling, with the context defaulting to its folder.
    assert_eq!(
        devcontainer_plan(
            ".devcontainer/devcontainer.json",
            r#"{"dockerFile": "../Dockerfile"}"#
        )
        .unwrap(),
        Plan::Build {
            dockerfile: "Dockerfile".into(),
            context: ".devcontainer".into(),
        }
    );
    assert_eq!(
        devcontainer_plan(
            ".devcontainer.json",
            r#"{"image": "mcr.microsoft.com/devcontainers/rust:1"}"#
        )
        .unwrap(),
        Plan::Image("mcr.microsoft.com/devcontainers/rust:1".into())
    );
}

#[test]
fn unusable_dev_containers_say_why() {
    let path = ".devcontainer/devcontainer.json";
    assert!(matches!(
        devcontainer_plan(path, r#"{"dockerComposeFile": "compose.yml"}"#),
        Err(Error::Unsupported(Unsupported::Compose))
    ));
    assert!(matches!(
        devcontainer_plan(path, "{}"),
        Err(Error::Unsupported(Unsupported::NoBuild))
    ));
    assert!(matches!(
        devcontainer_plan(path, r#"{"image": "--privileged"}"#),
        Err(Error::Field(Field::DevcontainerImage))
    ));
    assert!(matches!(
        devcontainer_plan(path, r#"{"build": {"dockerfile": "../../etc/Dockerfile"}}"#),
        Err(Error::Field(Field::ProjectPath))
    ));
    assert!(matches!(
        devcontainer_plan(path, "{"),
        Err(Error::Devcontainer(_))
    ));
}

#[test]
fn paths_stay_inside_the_project() {
    assert_eq!(join(".devcontainer", "..").unwrap(), ".");
    assert_eq!(join("a/b", "../c/./d").unwrap(), "a/c/d");
    assert!(join("a", "../..").is_err());
    assert!(join("", "/etc").is_err());
}

#[test]
fn comments_inside_strings_survive() {
    assert_eq!(
        strip_jsonc(r#"{"url": "https://x//y", "s": "a,}" /* c */, }"#),
        r#"{"url": "https://x//y", "s": "a,}"  }"#
    );
}
