use super::*;
use crate::usage::{probe::Shell, providers::codex};
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

const OPENCODE: &str = r#"{"openai":{"type":"oauth","access":"opencode-fixture-token",
    "accountId":"opencode-fixture-account","refresh":"unused-refresh-fixture","expires":1},
    "anthropic":{"type":"oauth","access":"unrelated-fixture-token"}}"#;
const CODEX: &str = r#"{"tokens":{"access_token":"codex-fixture-token",
    "account_id":"codex-fixture-account"}}"#;

/// Runs the public provider against an isolated host, with curl replaced by a
/// recorder. No user credentials, environment, or network are used.
struct HostFixture {
    // The shell is killed and waited for before its home is removed.
    exec: Exec,
    root: tempfile::TempDir,
    jar: CookieJar,
    auth_path: std::path::PathBuf,
}

impl HostFixture {
    fn new(codex_auth: Option<&str>, opencode_auth: Option<&str>, custom_data: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let bin = home.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        if let Some(auth) = codex_auth {
            write(&home.join(".codex/auth.json"), auth);
        }
        let data = if custom_data {
            home.join("custom data")
        } else {
            home.join(".local/share")
        };
        let auth_path = data.join("opencode/auth.json");
        if let Some(auth) = opencode_auth {
            write(&auth_path, auth);
        }
        if custom_data {
            write(
                &home.join(".local/share/opencode/auth.json"),
                r#"{"openai":{"type":"oauth","access":"wrong-data-directory"}}"#,
            );
        }
        write(&home.join("status"), "200");
        crate::test_executable::write(
            bin.join("curl"),
            concat!(
                "#!/bin/sh\n",
                "printf 'args: %s\\n' \"$*\" >> \"$HOME/requests\"\n",
                "cat <&3 >> \"$HOME/requests\"\n",
                "printf '%s' '{\"plan_type\":\"plus\",\"rate_limit\":{\"primary_window\":{\"used_percent\":23,\"limit_window_seconds\":18000}}}'\n",
                "printf '\\n@@herdr-status %s' \"$(cat \"$HOME/status\")\"\n",
            ),
            0o755,
        ).unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .arg("-s")
            .env_clear()
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if custom_data {
            command.env("XDG_DATA_HOME", &data);
        }
        Self {
            root,
            exec: Exec::Remote(Shell::start(command).unwrap()),
            jar: CookieJar::default(),
            auth_path,
        }
    }

    fn fetch(&mut self) -> Option<crate::Result<Report>> {
        let codex = provider("codex");
        let mut probe = Probe::new(&mut self.exec, codex, None, &mut self.jar, Consent::Quiet);
        codex.service().fetch(&mut probe)
    }

    fn requests(&self) -> String {
        fs::read_to_string(self.root.path().join("home/requests")).unwrap_or_default()
    }

    fn with_json_parser(&mut self, parser: Option<&str>) {
        let Exec::Remote(shell) = &mut self.exec else {
            unreachable!();
        };
        // Keep the old fallback's tools and the curl recorder available so a
        // missing utility cannot disguise an attempted cross-provider request.
        let output = shell
            .run(
                &format!(
                    r#"for tool in cat sed head tr {parser}; do
    [ -n "$tool" ] || continue
    source=$(command -v "$tool") || exit 1
    ln -s "$source" "$HOME/.local/bin/$tool" || exit 1
done
PATH="$HOME/.local/bin"
command -v curl"#,
                    parser = crate::usage::probe::quote(parser.unwrap_or_default()),
                ),
                Duration::from_secs(5),
            )
            .unwrap();
        assert!(output.success);
    }
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn opencode_oauth_reads_plan_usage_on_the_selected_host() {
    for (parser, custom_data) in [
        ("python3", false),
        ("python3", true),
        ("jq", false),
        ("jq", true),
    ] {
        let mut host = HostFixture::new(None, Some(OPENCODE), custom_data);
        host.with_json_parser(Some(parser));
        let report = host.fetch().unwrap().unwrap();
        assert_eq!(report.provider, provider("codex"));
        assert_eq!(report.account.plan.as_deref(), Some("Plus"));
        assert_eq!(report.windows[0].kind, Kind::Session);
        assert_eq!(report.windows[0].percent(), 23);
        let requests = host.requests();
        assert!(requests.contains(codex::URL));
        assert!(requests.contains("Authorization: Bearer opencode-fixture-token"));
        assert!(requests.contains("ChatGPT-Account-Id: opencode-fixture-account"));
        assert!(!requests.contains("unused-refresh-fixture"));
        assert!(!requests.contains("unrelated-fixture-token"));
        assert!(!requests.contains("wrong-data-directory"));
        assert_eq!(fs::read_to_string(&host.auth_path).unwrap(), OPENCODE);
        for line in requests.lines().filter(|line| line.starts_with("args: ")) {
            assert!(
                !line.contains("fixture"),
                "credentials belong in curl's input, not its arguments"
            );
        }
    }
}

#[test]
fn codex_oauth_takes_precedence_without_switching_accounts_on_rejection() {
    let mut host = HostFixture::new(Some(CODEX), Some(OPENCODE), false);
    host.fetch().unwrap().unwrap();
    assert!(
        host.requests()
            .contains("Authorization: Bearer codex-fixture-token")
    );
    assert!(
        host.requests()
            .contains("ChatGPT-Account-Id: codex-fixture-account")
    );
    write(&host.root.path().join("home/status"), "401");
    assert!(matches!(host.fetch(), Some(Err(Error::UsageRejected))));
    assert!(!host.requests().contains("opencode-fixture"));
    assert_eq!(host.requests().matches("args: ").count(), 2);
}

#[test]
fn missing_or_unusable_codex_auth_falls_back_to_opencode() {
    for auth in [
        None,
        Some("not json"),
        Some(r#"{"OPENAI_API_KEY":"api-fixture"}"#),
        Some(r#"{"tokens":{"access_token":""}}"#),
    ] {
        let mut host = HostFixture::new(auth, Some(OPENCODE), false);
        host.fetch().unwrap().unwrap();
        assert!(
            host.requests()
                .contains("Authorization: Bearer opencode-fixture-token")
        );
    }
}

#[test]
fn missing_malformed_and_non_oauth_opencode_entries_make_no_request() {
    let cases = [
        None,
        Some("not json"),
        Some("{}"),
        Some(r#"{"openai":{"type":"api","key":"api-fixture","access":"not-oauth"}}"#),
        Some(r#"{"openai":{"access":"missing-type"}}"#),
        Some(r#"{"openai":{"type":"oauth"}}"#),
        Some(r#"{"openai":{"type":"oauth","access":""}}"#),
        Some(r#"{"openai":{"type":"oauth","access":null}}"#),
        Some(r#"{"openai":{"type":"oauth","access":{}}}"#),
        Some(r#"{"openai":{"type":"oauth","access":[]}}"#),
        Some(r#"{"openai":{"type":"oauth","access":["not-a-token"]}}"#),
        Some(r#"{"anthropic":{"type":"oauth","access":"wrong-provider"}}"#),
    ];
    for parser in ["python3", "jq"] {
        for auth in cases {
            let mut host = HostFixture::new(None, auth, false);
            host.with_json_parser(Some(parser));
            assert!(host.fetch().is_none(), "{parser}: {auth:?}");
            assert!(host.requests().is_empty(), "{parser}: {auth:?}");
        }
    }
}

#[test]
fn opencode_auth_without_a_json_parser_never_uses_another_providers_token() {
    for openai in [
        Some(serde_json::json!({"type": "oauth", "access": "openai-fixture-token"})),
        Some(serde_json::json!({"type": "api", "key": "openai-fixture-key"})),
        None,
    ] {
        let mut auth = serde_json::json!({
            "anthropic": {"type": "oauth", "access": "anthropic-fixture-token"}
        });
        if let Some(openai) = openai {
            auth["openai"] = openai;
        }
        // A pretty-printed multi-provider file exposes the old last-key match:
        // Anthropic's OAuth entry precedes (or replaces) the OpenAI entry.
        let auth = serde_json::to_string_pretty(&auth).unwrap();
        let mut host = HostFixture::new(None, Some(&auth), false);
        host.with_json_parser(None);
        assert!(host.fetch().is_none());
        assert!(host.requests().is_empty());
    }
}

#[test]
fn account_id_is_optional_and_each_refresh_reads_opencodes_current_token() {
    let mut host = HostFixture::new(None, Some(OPENCODE), false);
    host.fetch().unwrap().unwrap();
    write(
        &host.auth_path,
        r#"{"openai":{"type":"oauth","access":"renewed-fixture-token"}}"#,
    );
    host.fetch().unwrap().unwrap();
    let requests = host.requests();
    let latest = requests.rsplit("args: ").next().unwrap();
    assert!(latest.contains("Authorization: Bearer renewed-fixture-token"));
    assert!(!latest.contains("ChatGPT-Account-Id"));
    assert!(!latest.contains("opencode-fixture-token"));
}
