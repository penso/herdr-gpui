use super::*;

const ID: &str = "0123456789abcdef0123456789abcdef";
const OTHER: &str = "fedcba9876543210fedcba9876543210";

#[test]
fn ssh_commands_reject_option_like_and_unterminated_commands() {
    // An executable cannot be an option, and an unterminated quote is refused
    // rather than ever executed. Both are valid TOML, so only the command
    // validation can refuse them.
    for text in [
        format!("[devices.{ID}]\nssh_command = '-F config'"),
        format!("[devices.{ID}]\nssh_command = \"/opt/ssh -F 'unclosed\""),
    ] {
        assert!(Config::parse(&text).is_err(), "accepted {text:?}");
    }
}

#[test]
fn command_reset_preserves_other_device_settings_and_refuses_invalid_ids() -> anyhow::Result<()> {
    let endpoint = format!("ssh:{ID}");
    let command = herdr_client::SshCommand::try_from("tailscale ssh")?;
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n[usage]\nshow = false\n";
    fs::write(&path, original)?;
    Config::save_device_ssh_command_path(ID, &command, &path)?;
    let saved = fs::read_to_string(&path)?;
    assert!(saved.starts_with(original), "{saved}");
    let config = Config::parse(&saved)?;
    assert_eq!(config.ssh_command(&endpoint).as_str(), "tailscale ssh");
    assert!(!config.usage.show);
    // A reset scopes to this device's command: its other preference keys and
    // another device's command survive.
    let shared = format!(
        "[devices.{ID}]\nkeybindings = 'server'\nssh_command = 'tailscale ssh'\n[devices.{OTHER}]\nssh_command = 'tailscale ssh'\n"
    );
    fs::write(&path, &shared)?;
    Config::save_device_ssh_command_path(ID, &herdr_client::SshCommand::default(), &path)?;
    let kept = fs::read_to_string(&path)?;
    let config = Config::parse(&kept)?;
    assert_eq!(
        config.keybinding_source(&endpoint),
        KeybindingSource::Server
    );
    assert!(config.ssh_command(&endpoint).is_default());
    assert_eq!(
        config.ssh_command(&format!("ssh:{OTHER}")).as_str(),
        "tailscale ssh"
    );
    // An ID that is not a catalog profile never reaches the file.
    assert!(matches!(
        Config::save_device_ssh_command_path("../x", &command, &path),
        Err(Error::InvalidDeviceId(_))
    ));
    assert_eq!(fs::read_to_string(&path)?, kept);
    Ok(())
}
