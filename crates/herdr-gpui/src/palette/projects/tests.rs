use super::*;

#[test]
fn expands_paths_without_shell_execution_and_rejects_unset_variables() -> anyhow::Result<()> {
    let root = if cfg!(windows) {
        "C:/projects"
    } else {
        "/projects"
    };
    let home = if cfg!(windows) {
        "C:/Users/me"
    } else {
        "/home/me"
    };
    let get = |name: &str| (name == "ROOT").then(|| OsString::from(root));
    assert_eq!(
        expand("$ROOT/work", get, None)?,
        Path::new(root).join("work")
    );
    assert_eq!(
        expand("${ROOT}/work", get, None)?,
        Path::new(root).join("work")
    );
    assert_eq!(
        expand("~/work", get, Some(Path::new(home)))?,
        Path::new(home).join("work")
    );
    assert!(matches!(
        expand("$MISSING/work", get, None),
        Err(crate::Error::Palette(Error::ProjectVariable(_)))
    ));
    assert!(matches!(
        expand("relative", get, None),
        Err(crate::Error::Palette(Error::ProjectPath))
    ));
    assert!(expand("${ROOT", get, None).is_err());
    assert!(expand("$(touch x)", get, None).is_err());
    Ok(())
}

#[test]
fn discovers_only_immediate_visible_folders_and_deduplicates_roots() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("alpha/nested"))?;
    fs::create_dir(root.path().join(".hidden"))?;
    fs::write(root.path().join("file"), "")?;
    let missing = root.path().join("missing").to_string_lossy().into_owned();
    let name = root.path().to_string_lossy().into_owned();
    let result = collect(&[missing, name.clone(), name], &AtomicBool::new(false));
    assert_eq!(result.projects.len(), 1);
    assert_eq!(result.projects[0].label, "alpha");
    assert_eq!(result.errors.len(), 1);
    use std::error::Error as _;
    assert!(result.errors[0].source().is_some());
    validate(&result.projects[0])?;
    let cancelled = collect(
        &[root.path().to_string_lossy().into_owned()],
        &AtomicBool::new(true),
    );
    assert!(cancelled.projects.is_empty());
    Ok(())
}

/// A root short enough for a plain path can hold children that are not:
/// one too long for it, and one whose name ends in a dot. Discovery lists
/// both in the form `validate` computes, so neither reads as removed.
#[cfg(windows)]
#[test]
fn discovered_children_validate_when_only_the_root_is_plain() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let long = "a".repeat(255);
    let verbatim = fs::canonicalize(root.path())?;
    fs::create_dir(verbatim.join(&long))?;
    fs::create_dir(verbatim.join("dot."))?;
    let result = collect(
        &[root.path().to_string_lossy().into_owned()],
        &AtomicBool::new(false),
    );
    assert!(result.errors.is_empty(), "{:?}", result.errors);
    assert_eq!(result.projects.len(), 2);
    for project in &result.projects {
        assert!(
            project.path.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            project.path.display()
        );
        validate(project)?;
    }
    Ok(())
}

#[test]
fn limits_filesystem_work() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    for index in 0..=MAX_PROJECTS {
        fs::create_dir(root.path().join(format!("project-{index}")))?;
    }
    let result = collect(
        &[root.path().to_string_lossy().into_owned()],
        &AtomicBool::new(false),
    );
    assert_eq!(result.projects.len(), MAX_PROJECTS);
    assert!(matches!(
        result.errors.as_slice(),
        [crate::Error::Palette(Error::ProjectLimit)]
    ));
    Ok(())
}

#[test]
fn matches_only_the_root_panes_launch_directory() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let path = super::super::canonical::canonicalize(root.path())?;
    let nested = path.join("nested");
    fs::create_dir(&nested)?;
    let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))?;
    snapshot.panes[0].cwd = Some(path.to_string_lossy().into_owned());
    snapshot.panes[0].foreground_cwd = Some(nested.to_string_lossy().into_owned());
    assert_eq!(workspace_for_path(&snapshot, &path), Some("w1".into()));
    assert_eq!(workspace_for_path(&snapshot, &nested), None);
    let mut split = snapshot.panes[0].clone();
    split.pane_id = "split".into();
    split.cwd = Some(nested.to_string_lossy().into_owned());
    snapshot.panes.push(split);
    assert_eq!(workspace_for_path(&snapshot, &nested), None);
    Ok(())
}

#[cfg(unix)]
#[test]
fn skips_child_symlinks_and_rejects_a_replaced_directory() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    std::os::unix::fs::symlink(outside.path(), root.path().join("link"))?;
    let path = root.path().join("project");
    fs::create_dir(&path)?;
    let result = collect(
        &[root.path().to_string_lossy().into_owned()],
        &AtomicBool::new(false),
    );
    assert_eq!(result.projects.len(), 1);
    fs::remove_dir(&path)?;
    std::os::unix::fs::symlink(outside.path(), &path)?;
    assert!(matches!(
        validate(&result.projects[0]),
        Err(crate::Error::Palette(Error::ProjectRemoved))
    ));
    Ok(())
}
