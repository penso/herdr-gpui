use super::*;

#[test]
fn the_vs_code_server_is_one_web_address() -> anyhow::Result<()> {
    assert_eq!(Config::parse("")?.code.url, None);
    assert_eq!(Config::parse(DEFAULT_CONFIG)?.code.url, None);
    let config = Config::parse("[code]\nurl = \"http://127.0.0.1:8000/?tkn=x\"")?;
    assert_eq!(
        config.code.url.as_ref().map(crate::browser::WebUrl::as_str),
        Some("http://127.0.0.1:8000/?tkn=x")
    );
    for invalid in [
        "[code]\nurl = \"file:///etc/passwd\"",
        "[code]\nurl = \"127.0.0.1:8000\"",
        "[code]\nurl = 8000",
        "[code]\nwidth = 400",
    ] {
        assert!(Config::parse(invalid).is_err(), "{invalid}");
    }
    Ok(())
}

#[test]
fn the_server_address_is_saved_and_removed_keeping_the_rest() -> anyhow::Result<()> {
    let temp = TempDirectory::new()?;
    let path = temp.0.join("config.toml");
    let url = crate::browser::WebUrl::try_from("http://127.0.0.1:8000/?tkn=x")?;
    // A missing file starts from the local template.
    Config::save_code_path(&CodeEdit::Url(Some(url.clone())), &path)?;
    let saved = Config::parse(&fs::read_to_string(&path)?)?;
    assert_eq!(saved.code.url.as_ref(), Some(&url));

    let original = "theme = 'Nord' # keep\n[code]\nurl = 'http://127.0.0.1:1/' # server\n";
    fs::write(&path, original)?;
    Config::save_code_path(&CodeEdit::Url(Some(url.clone())), &path)?;
    assert_eq!(
        fs::read_to_string(&path)?,
        original.replace("'http://127.0.0.1:1/'", "\"http://127.0.0.1:8000/?tkn=x\"")
    );
    // Removing the address drops the table it leaves empty.
    Config::save_code_path(&CodeEdit::Url(None), &path)?;
    assert_eq!(fs::read_to_string(&path)?, "theme = 'Nord' # keep\n");
    Config::save_code_path(&CodeEdit::Url(None), &path)?;
    assert_eq!(fs::read_to_string(&path)?, "theme = 'Nord' # keep\n");

    fs::write(&path, "code = 'yes'\n")?;
    let error = Config::save_code_path(&CodeEdit::Url(Some(url.clone())), &path)
        .err()
        .context("a code key that is not a table must be rejected")?;
    assert!(
        matches!(&error, Error::Path { source, .. } if matches!(**source, Error::InvalidCodeTable)),
        "{error:?}"
    );
    Ok(())
}
