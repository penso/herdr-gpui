//! The status bar's "Report issue" link: a GitHub bug report prefilled with
//! the versions, platform, install method, and current error.

use super::HerdrWindow;
use crate::{APP_VERSION, RELEASE_BUILD};
use gpui::Context;
use std::{path::Path, sync::OnceLock};

/// The longest error text sent in the report's `logs` field, in characters.
const LOGS_LIMIT: usize = 1024;

/// The platform description, gathered once: it does not change while the app
/// runs, and on macOS it waits on `ioreg` and `sysctl`.
static PLATFORM: OnceLock<String> = OnceLock::new();

impl HerdrWindow {
    /// Opens the prefilled bug report. The platform is gathered on the
    /// background executor because it runs child processes, so the window
    /// stays responsive while they finish. The versions and error are taken
    /// now, so a reconnect or host switch during that wait cannot clear or
    /// replace the error the user clicked to report.
    pub(super) fn open_report_issue(&self, cx: &mut Context<Self>) {
        let report = self.report_issue();
        let install_method = std::env::current_exe()
            .ok()
            .and_then(|exe| install_method(&exe, RELEASE_BUILD));
        let platform = cx
            .background_executor()
            .spawn(async { PLATFORM.get_or_init(report_issue_os).as_str() });
        cx.spawn(async move |_, cx| {
            let os = platform.await;
            cx.update(|cx| cx.open_url(&report.url(os, install_method)));
        })
        .detach();
    }

    /// The window state a bug report describes, read at one moment.
    pub(super) fn report_issue(&self) -> ReportIssue {
        let daemon_version = self.live.daemon_version().or_else(|| {
            self.live
                .missing_installation
                .then_some("none (not installed)")
        });
        let logs = self
            .live
            .error
            .as_deref()
            .or(self.local_error.as_deref())
            .map(|err| format!("{}: {err}", self.live.status));
        ReportIssue {
            version: client_version(RELEASE_BUILD, crate::constants::BUILD_COMMIT),
            daemon_version: daemon_version.map(str::to_owned),
            logs,
        }
    }
}

/// The versions and error captured when the report was requested, owned so
/// they can wait for the platform description without borrowing the window.
pub(super) struct ReportIssue {
    version: String,
    daemon_version: Option<String>,
    logs: Option<String>,
}

impl ReportIssue {
    /// A prefilled bug report URL with the client version, daemon version,
    /// the given platform and install method, and the error if any.
    pub(super) fn url(&self, os: &str, install_method: Option<&str>) -> String {
        report_issue_url(
            &self.version,
            self.daemon_version.as_deref(),
            os,
            install_method,
            self.logs.as_deref(),
        )
    }
}

/// The client version for bug reports, followed by the git commit for
/// source builds when it is known (e.g. "dev (2f643d73)").
pub(super) fn client_version(release: bool, commit: Option<&str>) -> String {
    match commit.filter(|commit| !release && !commit.is_empty()) {
        Some(commit) => format!("{APP_VERSION} ({commit})"),
        None => APP_VERSION.to_owned(),
    }
}

/// The running system's OS and hardware for the bug report template.
fn report_issue_os() -> String {
    let os = sysinfo::System::long_os_version().unwrap_or_else(|| match std::env::consts::OS {
        "macos" => "macOS".to_owned(),
        "linux" => "Linux".to_owned(),
        "windows" => "Windows".to_owned(),
        other => other.to_owned(),
    });
    let mut sys = sysinfo::System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    let chip = sys.cpus().first().map(|cpu| cpu.brand().to_owned());
    let model = mac_hardware_model();
    let arch = sysinfo::System::cpu_arch();
    let hardware = Hardware {
        chip: chip.as_deref(),
        memory_bytes: sys.total_memory(),
        model: model.as_deref(),
        os: std::env::consts::OS,
        arch: &arch,
    };
    format!("{os}, {}", hardware.describe())
}

/// What the report knows about the machine, kept apart from its detection
/// so the description can be tested with fixed inputs.
pub(super) struct Hardware<'a> {
    pub(super) chip: Option<&'a str>,
    pub(super) memory_bytes: u64,
    pub(super) model: Option<&'a str>,
    /// `std::env::consts::OS`.
    pub(super) os: &'a str,
    pub(super) arch: &'a str,
}

impl Hardware<'_> {
    /// The chip (e.g. `M1 Pro`), memory (e.g. `32GB RAM`), and model (e.g.
    /// `14" MacBook Pro`) that are known, or the architecture when none are.
    pub(super) fn describe(&self) -> String {
        let chip = self
            .chip
            .map(clean_cpu_brand)
            .filter(|chip| !chip.is_empty());
        let gigabytes = (self.memory_bytes as f64 / f64::from(1u32 << 30)).round() as u64;
        let memory = (gigabytes > 0).then(|| format!("{gigabytes}GB RAM"));
        let parts: Vec<String> = chip
            .into_iter()
            .chain(memory)
            .chain(self.model.map(str::to_owned))
            .collect();
        if !parts.is_empty() {
            return parts.join(", ");
        }
        match (self.os, self.arch) {
            ("macos", "arm64" | "aarch64") => "Apple Silicon".to_owned(),
            ("macos", "x86_64") => "Intel".to_owned(),
            (_, arch) => arch.to_owned(),
        }
    }
}

pub(super) fn clean_cpu_brand(brand: &str) -> String {
    let trimmed = brand.trim();
    if let Some(stripped) = trimmed.strip_prefix("Apple ") {
        return stripped.to_owned();
    }
    let cleaned = trimmed
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(tm)", "");
    let without_freq = if let Some((base, _)) = cleaned.split_once(" CPU @") {
        base
    } else if let Some((base, _)) = cleaned.split_once(" 12-Core") {
        base
    } else {
        &cleaned
    };
    without_freq
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "macos")]
fn mac_hardware_model() -> Option<String> {
    if let Ok(output) = herdr_client::new_command("ioreg")
        .args(["-rd1", "-c", "IOPlatformDevice", "-n", "product"])
        .output()
        && output.status.success()
        && let Ok(text) = String::from_utf8(output.stdout)
    {
        for line in text.lines() {
            if line.contains("product-name")
                && let Some(name) = extract_ioreg_string(line)
            {
                return Some(format_mac_model(&name));
            }
        }
    }
    if let Ok(output) = herdr_client::new_command("sysctl")
        .args(["-n", "hw.model"])
        .output()
        && output.status.success()
    {
        let model = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !model.is_empty() {
            return Some(format_mac_model(&model));
        }
    }
    None
}

#[cfg(not(target_os = "macos"))]
fn mac_hardware_model() -> Option<String> {
    None
}

#[cfg(any(test, target_os = "macos"))]
pub(super) fn extract_ioreg_string(line: &str) -> Option<String> {
    let (_, rest) = line.split_once('=')?;
    let trimmed = rest.trim();
    if let Some(inner) = trimmed
        .strip_prefix("<\"")
        .and_then(|s| s.strip_suffix("\">"))
    {
        return Some(inner.to_owned());
    }
    if let Some(inner) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Some(inner.to_owned());
    }
    None
}

#[cfg(any(test, target_os = "macos"))]
pub(super) fn format_mac_model(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some((device, paren_part)) = trimmed.split_once(" (")
        && let Some(inside) = paren_part.strip_suffix(')')
    {
        for token in inside.split(',') {
            let token = token.trim();
            if let Some(inches) = token.strip_suffix("-inch")
                && inches.chars().all(|c| c.is_ascii_digit())
            {
                return format!("{inches}\" {device}");
            }
        }
    }
    trimmed.to_owned()
}

/// How the executable at `exe` was installed, when that can be told.
///
/// A release build is left unset: the template's choices are both source
/// builds, and release packaging also produces an app bundle, so the user
/// picks the answer rather than the report claiming a local build.
pub(super) fn install_method(exe: &Path, release: bool) -> Option<&'static str> {
    if release {
        return None;
    }
    if exe
        .ancestors()
        .any(|p| p.extension().is_some_and(|ext| ext == "app"))
    {
        return Some("Locally built app bundle");
    }
    let in_target =
        exe.ancestors()
            .skip(1)
            .zip(exe.ancestors().skip(2))
            .any(|(profile, target)| {
                target.file_name().is_some_and(|name| name == "target")
                    && profile
                        .file_name()
                        .is_some_and(|name| name == "debug" || name == "release")
            });
    in_target.then_some("cargo run / just run")
}

/// Builds the GitHub issue URL with prefilled query parameters.
pub(super) fn report_issue_url(
    version: &str,
    daemon_version: Option<&str>,
    os: &str,
    install_method: Option<&str>,
    logs: Option<&str>,
) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("template", "bug_report.yml");
    serializer.append_pair("version", version);
    if let Some(daemon) = daemon_version {
        serializer.append_pair("daemon_version", daemon);
    }
    serializer.append_pair("os", os);
    if let Some(method) = install_method {
        serializer.append_pair("install_method", method);
    }
    if let Some(log) = logs {
        serializer.append_pair("logs", &crate::truncate::ellipsize(log.trim(), LOGS_LIMIT));
    }
    format!(
        "https://github.com/penso/herdr-gpui/issues/new?{}",
        serializer.finish()
    )
}
