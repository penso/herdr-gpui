#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn a_checkout_is_the_folder_holding_its_git_directory() {
    assert_eq!(checkout_of("/src/app/.git"), Some("/src/app"));
    assert_eq!(checkout_of("/src/app/.git/"), Some("/src/app"));
    assert_eq!(checkout_of("/srv/app.git"), None, "a bare repository");
    assert_eq!(checkout_of("/.git"), None);
}

#[cfg(unix)]
mod reading {
    use super::*;
    use std::{fs, path::Path, process::Command};

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["init", "-q"])
            .arg(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        dir
    }

    fn read_at(root: &Path) -> Option<Recipe> {
        let host = crate::teleport::Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
        let checkout = Checkout {
            host: &host,
            endpoint_id: "local",
            path: root.to_str().unwrap(),
            label: "app",
        };
        read(&checkout, &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn a_project_without_a_recipe_has_no_image() {
        let dir = repository();
        assert!(read_at(dir.path()).is_none());
    }

    #[test]
    fn the_herdr_recipe_wins_and_sends_only_what_it_copies() {
        let dir = repository();
        let sandbox = dir.path().join(".herdr/sandbox");
        fs::create_dir_all(&sandbox).unwrap();
        fs::write(
            sandbox.join("Dockerfile"),
            "FROM debian:12\nCOPY tools/ /opt/tools/\n",
        )
        .unwrap();
        fs::create_dir_all(sandbox.join("tools")).unwrap();
        fs::write(sandbox.join("tools/a.sh"), "echo a\n").unwrap();
        fs::write(sandbox.join("unrelated.txt"), "not copied\n").unwrap();
        fs::create_dir_all(dir.path().join(".devcontainer")).unwrap();
        fs::write(
            dir.path().join(".devcontainer/devcontainer.json"),
            r#"{"image": "ignored:1"}"#,
        )
        .unwrap();
        let recipe = read_at(dir.path()).unwrap();
        assert!(recipe.dockerfile.starts_with("FROM debian:12\n"));
        assert!(recipe.dockerfile.contains("HERDR_INSTALL_DIR"));
        let paths: Vec<_> = recipe.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, ["tools/a.sh"]);
        assert_eq!(recipe.project.label, "app");
        assert_eq!(recipe.origin.path, dir.path().to_str().unwrap());
    }

    #[test]
    fn a_dev_container_names_the_image_or_dockerfile() {
        let dir = repository();
        fs::create_dir_all(dir.path().join(".devcontainer")).unwrap();
        fs::write(
            dir.path().join(".devcontainer/devcontainer.json"),
            "{ // the base\n \"image\": \"mcr.microsoft.com/devcontainers/base:bookworm\" }",
        )
        .unwrap();
        let recipe = read_at(dir.path()).unwrap();
        assert!(
            recipe
                .dockerfile
                .starts_with("FROM mcr.microsoft.com/devcontainers/base:bookworm\n")
        );
        assert!(recipe.files.is_empty());
        fs::write(
            dir.path().join(".devcontainer/devcontainer.json"),
            r#"{"build": {"dockerfile": "Dockerfile.dev", "context": ".."}}"#,
        )
        .unwrap();
        assert!(matches!(
            read(
                &Checkout {
                    host: &crate::teleport::Host::new(&sheprd_client::ConnectTarget::Local).unwrap(),
                    endpoint_id: "local",
                    path: dir.path().to_str().unwrap(),
                    label: "app",
                },
                &AtomicBool::new(false)
            ),
            Err(Error::Missing(path)) if path == ".devcontainer/Dockerfile.dev"
        ));
        fs::write(
            dir.path().join(".devcontainer/Dockerfile.dev"),
            "FROM rust:1\nCOPY Cargo.toml /\n",
        )
        .unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let recipe = read_at(dir.path()).unwrap();
        assert_eq!(recipe.files.len(), 1);
        assert_eq!(recipe.files[0].path, "Cargo.toml");
    }
}
