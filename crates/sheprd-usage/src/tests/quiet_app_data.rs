use super::*;
use crate::{probe::Shell, providers::muse, settings::ProviderSettings};

/// A remote Mac stood in for by a local `sh`: `uname` says Darwin, and each
/// named tool is a stub that records its arguments and prints `output`.
struct Mac {
    root: tempfile::TempDir,
}

impl Mac {
    fn new(tools: &[(&str, &str)]) -> Self {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("home/.local/bin");
        std::fs::create_dir_all(&bin).unwrap();
        crate::test_executable::write(bin.join("uname"), "#!/bin/sh\necho Darwin\n", 0o755)
            .unwrap();
        for (tool, output) in tools {
            crate::test_executable::write(
                bin.join(tool),
                format!(
                    "#!/bin/sh\nprintf '%s %s\\n' '{tool}' \"$*\" >> '{log}'\nprintf '%s' '{output}'\n",
                    log = log(root.path()).display()
                ),
                0o755,
            )
            .unwrap();
        }
        Self { root }
    }

    /// Creates `path` under the fake home, with its parent directories.
    fn file(&self, path: &str) {
        let path = self.root.path().join("home").join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    fn fetch(&self, id: &str, consent: Consent) -> Option<crate::Result<Report>> {
        self.probe(id, None, consent, |probe| {
            provider(id).service().fetch(probe)
        })
    }

    fn probe<T>(
        &self,
        id: &str,
        settings: Option<&ProviderSettings>,
        consent: Consent,
        run: impl FnOnce(&mut Probe) -> T,
    ) -> T {
        let mut command = std::process::Command::new("/bin/sh");
        command
            .arg("-s")
            .env_clear()
            .env("HOME", self.root.path().join("home"))
            .env("PATH", "/usr/bin:/bin")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());
        let mut exec = Exec::Remote(Shell::start(command).unwrap());
        let mut jar = CookieJar::default();
        let mut probe = Probe::new(&mut exec, provider(id), settings, &mut jar, consent);
        run(&mut probe)
    }

    fn runs(&self) -> String {
        std::fs::read_to_string(log(self.root.path())).unwrap_or_default()
    }
}

fn log(root: &std::path::Path) -> std::path::PathBuf {
    root.join("log")
}

const LISTED: Consent = Consent::Ask { browsers: false };

/// `muse login`'s keychain item is guarded by its own access list, so
/// `security` reading it makes macOS ask (#176). Like Zed's sign-in, a
/// detected Muse leaves it alone and a listed one waits for Allow.
#[test]
fn muse_reads_its_keychain_item_only_once_allowed() {
    const READ: &str = "security find-generic-password -s ai.meta.dev.credentials -w -a meta";
    let mac = Mac::new(&[
        ("security", r#"{"access_token":"dca:token"}"#),
        ("curl", "{}"),
    ]);
    assert!(mac.fetch("muse", Consent::Quiet).is_none());
    let missing = mac.probe("muse", None, LISTED, |probe| {
        (muse::token(probe).is_none(), probe.missing())
    });
    assert!(missing.0, "a listed Muse read the item before Allow");
    assert!(matches!(missing.1, Error::KeychainAccess));
    assert!(
        !mac.runs().contains("security"),
        "another app's keychain item was read without Allow: {}",
        mac.runs()
    );

    assert!(mac.fetch("muse", Consent::Keychain).is_some());
    assert!(
        mac.runs().contains(READ),
        "an allowed Muse no longer reads its keychain item: {}",
        mac.runs()
    );
}

/// The `token` setting asks nothing, so a detected Muse still uses it.
#[test]
fn a_detected_muse_still_uses_its_token_setting() {
    let mac = Mac::new(&[("security", r#"{"access_token":"dca:keychain"}"#)]);
    let settings = ProviderSettings::default().with("token", "dca:setting");
    let found = mac.probe("muse", Some(&settings), Consent::Quiet, |probe| {
        muse::token(probe).is_some()
    });
    assert!(found, "a detected Muse dropped its token setting");
    assert!(!mac.runs().contains("security"), "{}", mac.runs());
}

#[test]
fn claude_still_reads_its_keychain_item_when_only_detected() {
    let mac = Mac::new(&[("security", "{}")]);
    let _ = mac.fetch("claude", Consent::Quiet);
    assert!(
        mac.runs().contains("Claude Code-credentials"),
        "Claude Code's own item asks nothing and stays detectable: {}",
        mac.runs()
    );
}

/// Over SSH no dialog reaches the user, so a remote Mac's editor data is
/// read for a detected provider as before, and for a listed one too.
#[test]
fn a_remote_mac_still_reads_editor_data() {
    for consent in [Consent::Quiet, LISTED] {
        let mac = Mac::new(&[("sqlite3", "x"), ("kiro-cli", "")]);
        mac.file("Library/Application Support/Windsurf/User/globalStorage/state.vscdb");
        mac.file("Library/Application Support/kiro-cli/data.sqlite3");
        mac.file(
            "Library/Application Support/JetBrains/IntelliJIdea2025.3/options/AIAssistantQuotaManager2.xml",
        );
        for id in ["windsurf", "kiro", "jetbrains"] {
            assert!(
                mac.fetch(id, consent).is_some(),
                "{id} skipped a remote Mac's data under {consent:?}"
            );
        }
        let runs = mac.runs();
        assert!(runs.contains("sqlite3 -readonly"), "{runs}");
        assert!(
            runs.contains("kiro-cli chat --no-interactive /usage"),
            "{runs}"
        );
    }
}

/// On this machine a detected provider may not touch another app's data
/// where macOS guards it; anywhere else, and for a listed provider, it may.
#[test]
fn app_data_waits_for_show_providers_on_a_local_mac() {
    let mut jar = CookieJar::default();
    for consent in [
        Consent::Quiet,
        LISTED,
        Consent::Ask { browsers: true },
        Consent::Keychain,
        Consent::Browsers,
    ] {
        let mut exec = Exec::Local;
        let probe = Probe::new(&mut exec, provider("windsurf"), None, &mut jar, consent);
        let expected = consent != Consent::Quiet || !cfg!(target_os = "macos");
        assert_eq!(probe.app_data(), expected, "{consent:?}");
    }
}

/// Whatever this machine has installed, a detected Windsurf, JetBrains IDE,
/// or Kiro on this Mac is never read: the check comes before any file or
/// command, so nothing can ask.
#[cfg(target_os = "macos")]
#[test]
fn a_local_mac_skips_editor_data_for_detected_providers() {
    let mut jar = CookieJar::default();
    for id in ["windsurf", "kiro", "jetbrains"] {
        let mut exec = Exec::Local;
        let provider = provider(id);
        let mut probe = Probe::new(&mut exec, provider, None, &mut jar, Consent::Quiet);
        assert!(provider.service().fetch(&mut probe).is_none(), "{id}");
    }
}

/// A set `ide_path` is the user's own choice, so JetBrains reads it even when
/// only detected, on this Mac too.
#[test]
fn a_detected_jetbrains_still_reads_its_ide_path() {
    let root = tempfile::tempdir().unwrap();
    let ide = root.path().join("IntelliJIdea2025.3");
    std::fs::create_dir_all(ide.join("options")).unwrap();
    std::fs::write(ide.join("options/AIAssistantQuotaManager2.xml"), "x").unwrap();
    let settings = ProviderSettings::default().with("ide_path", ide.to_str().unwrap());
    let jetbrains = provider("jetbrains");
    let mut exec = Exec::Local;
    let mut jar = CookieJar::default();
    let mut probe = Probe::new(
        &mut exec,
        jetbrains,
        Some(&settings),
        &mut jar,
        Consent::Quiet,
    );
    assert!(
        jetbrains.service().fetch(&mut probe).is_some(),
        "ide_path was not read for a detected JetBrains"
    );
}
