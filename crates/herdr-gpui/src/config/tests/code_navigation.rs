use super::*;

#[test]
fn files_open_in_the_editor_unless_configured() -> anyhow::Result<()> {
    for config in [Config::parse("")?, Config::parse(DEFAULT_CONFIG)?] {
        assert_eq!(config.open_files_in, FileTarget::Editor);
        assert_eq!(config.editor_command, None);
    }
    assert_eq!(
        Config::parse("open_files_in = \"system\"")?.open_files_in,
        FileTarget::System
    );
    assert!(Config::parse("open_files_in = \"vim\"").is_err());
    let config = Config::parse("editor_command = \"hx {file}:{line}\"")?;
    assert_eq!(
        config.editor_command,
        Some(crate::editor::EditorCommand::try_from(
            "hx {file}:{line}".to_owned()
        )?)
    );
    // A command that would break out of its quoting is refused.
    assert!(Config::parse("editor_command = \"vim -c 'q'\"").is_err());
    Ok(())
}
