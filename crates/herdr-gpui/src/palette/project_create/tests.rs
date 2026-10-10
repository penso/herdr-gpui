use super::*;

#[test]
fn accepts_plain_folder_names() {
    assert_eq!(parse_name("my-project").as_deref(), Some("my-project"));
    assert_eq!(
        parse_name("  spaced name  ").as_deref(),
        Some("spaced name")
    );
    assert_eq!(parse_name("v2.0").as_deref(), Some("v2.0"));
}

#[test]
fn refuses_names_that_are_not_one_segment() {
    for bad in ["", "   ", ".", "..", ".hidden", "a/b", "a\\b", "a:b"] {
        assert_eq!(parse_name(bad), None, "should refuse {bad:?}");
    }
}

#[test]
fn recognises_explicit_repository_urls() {
    assert_eq!(
        parse_repo("https://github.com/owner/repo.git"),
        Some(("https://github.com/owner/repo.git".into(), "repo".into()))
    );
    assert_eq!(
        parse_repo("git@github.com:owner/repo.git"),
        Some(("git@github.com:owner/repo.git".into(), "repo".into()))
    );
    assert_eq!(
        parse_repo("ssh://git@host/owner/repo"),
        Some(("ssh://git@host/owner/repo".into(), "repo".into()))
    );
}

#[test]
fn refuses_a_bare_owner_repo() {
    // Choosing a forge for the user is not ours to do.
    assert_eq!(parse_repo("owner/repo"), None);
    assert_eq!(parse_repo("just-a-name"), None);
}

#[test]
fn plans_a_folder_in_the_first_root() -> anyhow::Result<()> {
    let root = std::env::var("HOME")?;
    let Some(plan) = plan(std::slice::from_ref(&root), "brand-new")? else {
        anyhow::bail!("expected a plan for a plain name");
    };
    assert_eq!(plan.path, PathBuf::from(&root).join("brand-new"));
    assert_eq!(plan.label, "brand-new");
    assert_eq!(plan.source, Source::Folder);
    Ok(())
}

#[test]
fn plans_a_clone_and_names_it_after_the_repository() -> anyhow::Result<()> {
    let root = std::env::var("HOME")?;
    let Some(plan) = plan(
        std::slice::from_ref(&root),
        "https://github.com/owner/thing.git",
    )?
    else {
        anyhow::bail!("expected a plan for a repository URL");
    };
    assert_eq!(plan.path, PathBuf::from(root).join("thing"));
    assert_eq!(
        plan.source,
        Source::Clone {
            url: "https://github.com/owner/thing.git".into()
        }
    );
    Ok(())
}

#[test]
fn plans_nothing_without_a_root() -> anyhow::Result<()> {
    assert_eq!(plan(&[], "anything")?, None);
    Ok(())
}

#[test]
fn the_clone_command_passes_url_and_destination_as_data() {
    let command = clone_command("https://example.test/a.git", Path::new("/tmp/a"));
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        args,
        vec![
            "-c",
            "core.fsmonitor=false",
            "clone",
            "--",
            "https://example.test/a.git",
            "/tmp/a"
        ]
    );
    let prompt = command
        .get_envs()
        .find(|(key, _)| *key == "GIT_TERMINAL_PROMPT")
        .and_then(|(_, value)| value)
        .map(|value| value.to_string_lossy().into_owned());
    assert_eq!(prompt.as_deref(), Some("0"));
}

#[test]
fn a_second_project_of_the_same_name_is_refused() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let project = NewProject {
        path: root.path().join("taken"),
        label: "taken".into(),
        source: Source::Folder,
    };
    fs::create_dir(&project.path)?;
    assert!(matches!(
        materialise(&project, &|| false),
        Err(Error::PaletteProjectExists)
    ));
    Ok(())
}

fn folder_names(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut names = fs::read_dir(root)?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<anyhow::Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

fn clone_project(root: &Path, url: &str) -> NewProject {
    NewProject {
        path: root.join("cloned"),
        label: "cloned".into(),
        source: Source::Clone { url: url.into() },
    }
}

#[test]
fn a_new_folder_opens_at_its_resolved_path() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let project = NewProject {
        path: root.path().join("fresh"),
        label: "fresh".into(),
        source: Source::Folder,
    };
    let opened = materialise(&project, &|| false)?;
    assert_eq!(opened.path, fs::canonicalize(root.path())?.join("fresh"));
    assert_eq!(opened.label, "fresh");
    assert!(opened.path.is_dir());
    Ok(())
}

#[test]
fn a_failed_clone_leaves_neither_the_project_nor_its_staging_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let missing = root.path().join("no-such-repository");
    let project = clone_project(root.path(), &missing.to_string_lossy());
    assert!(matches!(
        materialise(&project, &|| false),
        Err(Error::PaletteProjectClone(_))
    ));
    assert!(folder_names(root.path())?.is_empty());
    Ok(())
}

#[test]
fn a_clone_is_published_only_once_it_finishes() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let source = root.path().join("source");
    let init = Command::new("git")
        .args(["init", "--quiet", "--bare"])
        .arg(&source)
        .status()?;
    anyhow::ensure!(init.success(), "git init failed");
    let project = clone_project(root.path(), &source.to_string_lossy());
    let opened = materialise(&project, &|| false)?;
    assert!(opened.path.join(".git").is_dir());
    // No staging folder is left behind beside the published project.
    assert_eq!(folder_names(root.path())?, ["cloned", "source"]);
    Ok(())
}

#[test]
fn a_clone_does_not_start_when_the_name_is_taken() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("cloned"))?;
    let project = clone_project(root.path(), "https://example.invalid/cloned.git");
    assert!(matches!(
        materialise(&project, &|| false),
        Err(Error::PaletteProjectExists)
    ));
    assert_eq!(folder_names(root.path())?, ["cloned"]);
    Ok(())
}

#[test]
fn publishing_never_replaces_an_existing_folder() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let destination = root.path().join("taken");
    for saved in [false, true] {
        fs::create_dir_all(&destination)?;
        if saved {
            fs::write(destination.join("notes.txt"), "kept")?;
        }
        let staging = staging_dir(root.path(), "taken")?;
        fs::write(staging.path().join("README"), "clone")?;
        assert!(matches!(
            publish(staging.path(), &destination),
            Err(Error::PaletteProjectExists)
        ));
        assert!(!destination.join("README").exists());
        assert_eq!(saved, destination.join("notes.txt").exists());
    }
    Ok(())
}

#[test]
fn staging_folders_are_hidden_unique_and_removed_when_dropped() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let first = staging_dir(root.path(), "repo")?;
    let second = staging_dir(root.path(), "repo")?;
    assert_ne!(first.path(), second.path());
    for staging in [first.path(), second.path()] {
        assert_eq!(staging.parent(), Some(root.path()));
        let name = staging.file_name().map(|name| name.to_string_lossy());
        assert!(name.is_some_and(|name| name.starts_with(".repo.clone-")));
    }
    // Discovery skips dot folders, so an unfinished clone cannot be opened.
    let found = projects::collect(
        &[root.path().to_string_lossy().into_owned()],
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert!(found.projects.is_empty());
    drop((first, second));
    assert!(folder_names(root.path())?.is_empty());
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_published_clone_has_ordinary_folder_permissions() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir()?;
    let plain = root.path().join("plain");
    fs::create_dir(&plain)?;
    let staging = staging_dir(root.path(), "repo")?;
    let mode = |path: &Path| -> anyhow::Result<u32> {
        Ok(fs::metadata(path)?.permissions().mode() & 0o777)
    };
    assert_eq!(mode(staging.path())?, mode(&plain)?);
    Ok(())
}
