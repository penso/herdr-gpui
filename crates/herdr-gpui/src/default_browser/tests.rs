use super::*;

const URL: &str = "file:///X:/dev/x/.tmp/mocks/m/index.html?sheet=1#top";

fn parsed(template: &str, url: &str) -> (PathBuf, Vec<String>) {
    match command_line(template, url) {
        Ok(parsed) => parsed,
        Err(error) => panic!("{template:?} did not parse: {error}"),
    }
}

fn assert_unparsable(template: &str) {
    match command_line(template, URL) {
        Err(Error::UnparsableCommand { command }) => assert_eq!(command, template),
        other => panic!("{template:?} gave {other:?}, not UnparsableCommand"),
    }
}

#[test]
fn firefox_template_puts_the_url_after_its_flags() {
    let (program, args) = parsed(
        r#""C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1""#,
        URL,
    );
    assert_eq!(
        program,
        PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe")
    );
    assert_eq!(args, ["-osint", "-url", URL]);
}

#[test]
fn chrome_template_passes_the_url_as_its_single_argument() {
    let (program, args) = parsed(
        r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --single-argument %1"#,
        URL,
    );
    assert_eq!(
        program,
        PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe")
    );
    assert_eq!(args, ["--single-argument", URL]);
}

#[test]
fn edge_template_passes_the_url_as_its_single_argument() {
    let (program, args) = parsed(
        r#""C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe" --single-argument %1"#,
        URL,
    );
    assert_eq!(
        program,
        PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe")
    );
    assert_eq!(args, ["--single-argument", URL]);
}

#[test]
fn unquoted_program_ends_at_the_first_space() {
    let (program, args) = parsed(r"C:\Tools\browser.exe --new-tab %1", URL);
    assert_eq!(program, PathBuf::from(r"C:\Tools\browser.exe"));
    assert_eq!(args, ["--new-tab", URL]);
}

#[test]
fn long_name_placeholders_are_replaced_too() {
    for placeholder in ["%L", "%l"] {
        let (_, args) = parsed(&format!("browser.exe {placeholder}"), URL);
        assert_eq!(args, [URL], "{placeholder}");
    }
}

#[test]
fn placeholder_inside_an_argument_is_replaced_in_place() {
    let (_, args) = parsed("browser.exe --url=%1", URL);
    assert_eq!(args, [format!("--url={URL}")]);
}

#[test]
fn all_arguments_placeholder_is_dropped() {
    let (_, args) = parsed("browser.exe %1 %*", URL);
    assert_eq!(args, [URL]);
}

#[test]
fn template_without_placeholder_gets_the_url_appended() {
    let (program, args) = parsed(r#""C:\Apps\browser.exe" --new-window"#, URL);
    assert_eq!(program, PathBuf::from(r"C:\Apps\browser.exe"));
    assert_eq!(args, ["--new-window", URL]);
}

#[test]
fn url_with_quotes_and_spaces_stays_one_argument() {
    let url = r#"file:///C:/a b/x.html?q="1 2" -new -arg %L"#;
    let (_, args) = parsed(
        r#""C:\Program Files\Mozilla Firefox\firefox.exe" -osint -url "%1""#,
        url,
    );
    assert_eq!(args, ["-osint", "-url", url]);
}

#[test]
fn escaped_quotes_and_backslash_runs_follow_windows_rules() {
    let (_, args) = parsed(r#"browser.exe --name=a\"b "c:\dir\\" d\e %1"#, URL);
    assert_eq!(args, [r#"--name=a"b"#, r"c:\dir\", r"d\e", URL]);
}

#[test]
fn unterminated_quote_is_unparsable() {
    assert_unparsable(r#""C:\Program Files\browser.exe --flag %1"#);
    assert_unparsable(r#"browser.exe "%1"#);
}

#[test]
fn empty_template_or_program_is_unparsable() {
    assert_unparsable("");
    assert_unparsable("   ");
    assert_unparsable(r#""" %1"#);
}
