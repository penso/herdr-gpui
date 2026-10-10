use super::*;

#[test]
fn older_configs_keep_using_their_address() -> anyhow::Result<()> {
    // A config from before the choice: an address and nothing else.
    let config = Config::parse("[code]\nurl = \"http://127.0.0.1:8000/?tkn=x\"")?;
    assert_eq!(config.code.mode, None);
    assert_eq!(config.code.port, None);
    assert!(!config.code.license_accepted);
    assert!(config.code.url.is_some());
    assert_eq!(Config::parse("")?.code, CodeConfig::default());
    assert_eq!(Config::parse(DEFAULT_CONFIG)?.code, CodeConfig::default());
    Ok(())
}

#[test]
fn the_mode_port_and_license_are_read() -> anyhow::Result<()> {
    let config = Config::parse(
        "[code]\nmode = \"start\"\nport = 51234\nlicense_accepted = true\n\
         url = \"http://127.0.0.1:8000/?tkn=x\"",
    )?;
    assert_eq!(config.code.mode, Some(CodeMode::Start));
    assert_eq!(config.code.port.map(std::num::NonZeroU16::get), Some(51234));
    assert!(config.code.license_accepted);
    // The address is kept for switching back.
    assert!(config.code.url.is_some());
    assert_eq!(
        Config::parse("[code]\nmode = \"address\"")?.code.mode,
        Some(CodeMode::Address)
    );
    for invalid in [
        "[code]\nmode = \"auto\"",
        "[code]\nport = 0",
        "[code]\nport = 70000",
        "[code]\nlicense_accepted = \"yes\"",
    ] {
        assert!(Config::parse(invalid).is_err(), "{invalid}");
    }
    Ok(())
}

#[test]
fn each_choice_is_saved_alongside_the_address() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let original = "theme = 'Nord' # keep\n[code]\nurl = 'http://127.0.0.1:1/' # server\n";
    fs::write(&path, original)?;
    let port = std::num::NonZeroU16::new(51234).context("port")?;
    for edit in [
        CodeEdit::Mode(CodeMode::Start),
        CodeEdit::Port(port),
        CodeEdit::AcceptLicense,
    ] {
        Config::save_code_path(&edit, &path)?;
    }
    let saved = Config::parse(&fs::read_to_string(&path)?)?;
    assert_eq!(saved.code.mode, Some(CodeMode::Start));
    assert_eq!(saved.code.port, Some(port));
    assert!(saved.code.license_accepted);
    assert_eq!(
        saved.code.url.as_ref().map(crate::browser::WebUrl::as_str),
        Some("http://127.0.0.1:1/")
    );
    assert!(fs::read_to_string(&path)?.starts_with(original));

    Config::save_code_path(&CodeEdit::Mode(CodeMode::Address), &path)?;
    let text = fs::read_to_string(&path)?;
    assert!(text.contains("mode = \"address\""), "{text}");
    assert!(!text.contains("start"), "{text}");
    Ok(())
}
