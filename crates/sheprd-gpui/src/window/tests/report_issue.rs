use super::*;
use crate::window::report_issue::{
    Hardware, clean_cpu_brand, client_version, extract_ioreg_string, format_mac_model,
    install_method, report_issue_url,
};
use gpui::TestAppContext;
use std::path::Path;

const GIB: u64 = 1 << 30;

#[test]
fn report_issue_url_prefills_all_fields() {
    let url = report_issue_url(
        "20261015.1",
        Some("0.9.1"),
        "macOS 15.3.1 Sequoia, Apple Silicon",
        Some("Locally built app bundle"),
        Some("Disconnected: socket closed"),
    );
    assert_eq!(
        url,
        "https://github.com/sheprd-labs/sheprd/issues/new?template=bug_report.yml\
         &version=20261015.1\
         &daemon_version=0.9.1\
         &os=macOS+15.3.1+Sequoia%2C+Apple+Silicon\
         &install_method=Locally+built+app+bundle\
         &logs=Disconnected%3A+socket+closed"
    );
}

#[test]
fn report_issue_url_omits_optional_fields_when_absent() {
    let url = report_issue_url("dev", None, "Linux (Ubuntu 24.04), x86_64", None, None);
    assert_eq!(
        url,
        "https://github.com/sheprd-labs/sheprd/issues/new?template=bug_report.yml\
         &version=dev\
         &os=Linux+%28Ubuntu+24.04%29%2C+x86_64"
    );
    assert!(!url.contains("daemon_version="));
    assert!(!url.contains("install_method="));
    assert!(!url.contains("logs="));
}

#[test]
fn report_issue_url_encodes_plus_in_version() {
    let url = report_issue_url("20260920.1+abc", None, "macOS, Apple Silicon", None, None);
    assert!(url.contains("version=20260920.1%2Babc"));
}

#[test]
fn report_issue_url_bounds_large_logs() {
    let large_log = "error ".repeat(300);
    let url = report_issue_url("dev", None, "macOS, Apple Silicon", None, Some(&large_log));
    assert!(url.contains("logs="));
    // Must contain ellipsis
    assert!(url.contains("%E2%80%A6"));
    // Entire URL stays well bounded
    assert!(url.len() < 3000);
}

#[test]
fn hardware_lists_the_known_chip_memory_and_model() {
    let mac_mini = Hardware {
        chip: Some("Apple M2"),
        memory_bytes: 16 * GIB,
        model: Some("Macmini14,3"),
        os: "macos",
        arch: "arm64",
    };
    assert_eq!(mac_mini.describe(), "M2, 16GB RAM, Macmini14,3");
    let laptop = Hardware {
        chip: Some("Apple M1 Pro"),
        memory_bytes: 32 * GIB,
        model: Some("14\" MacBook Pro"),
        os: "macos",
        arch: "arm64",
    };
    assert_eq!(laptop.describe(), "M1 Pro, 32GB RAM, 14\" MacBook Pro");
    let linux = Hardware {
        chip: Some("AMD Ryzen 7 7800X3D"),
        memory_bytes: 0,
        model: None,
        os: "linux",
        arch: "x86_64",
    };
    assert_eq!(linux.describe(), "AMD Ryzen 7 7800X3D");
}

#[test]
fn hardware_falls_back_to_the_architecture() {
    let unknown = |os, arch| Hardware {
        chip: Some("  "),
        memory_bytes: 0,
        model: None,
        os,
        arch,
    };
    assert_eq!(unknown("macos", "arm64").describe(), "Apple Silicon");
    assert_eq!(unknown("macos", "x86_64").describe(), "Intel");
    assert_eq!(unknown("linux", "aarch64").describe(), "aarch64");
}

#[test]
fn clean_cpu_brand_strips_apple_and_intel_noise() {
    assert_eq!(clean_cpu_brand("Apple M1 Pro"), "M1 Pro");
    assert_eq!(clean_cpu_brand("Apple M2 Max"), "M2 Max");
    assert_eq!(
        clean_cpu_brand("Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz"),
        "Intel Core i7-9750H"
    );
    assert_eq!(
        clean_cpu_brand("AMD Ryzen 7 7800X3D"),
        "AMD Ryzen 7 7800X3D"
    );
}

#[test]
fn format_mac_model_converts_screen_sizes() {
    assert_eq!(
        format_mac_model("MacBook Pro (14-inch, 2021)"),
        "14\" MacBook Pro"
    );
    assert_eq!(
        format_mac_model("MacBook Air (15-inch, M2, 2023)"),
        "15\" MacBook Air"
    );
    assert_eq!(format_mac_model("iMac (24-inch, M1, 2021)"), "24\" iMac");
    assert_eq!(format_mac_model("Mac mini (2023)"), "Mac mini (2023)");
    assert_eq!(format_mac_model("MacBookPro18,3"), "MacBookPro18,3");
}

#[test]
fn extract_ioreg_string_handles_quotes_and_brackets() {
    assert_eq!(
        extract_ioreg_string("      \"product-name\" = <\"MacBook Pro (14-inch, 2021)\">"),
        Some("MacBook Pro (14-inch, 2021)".to_owned())
    );
    assert_eq!(
        extract_ioreg_string("      \"product-name\" = \"MacBook Pro (14-inch, 2021)\""),
        Some("MacBook Pro (14-inch, 2021)".to_owned())
    );
    assert_eq!(extract_ioreg_string("invalid line"), None);
}

#[test]
fn client_version_names_the_commit_of_source_builds_only() {
    let app = crate::APP_VERSION;
    assert_eq!(
        client_version(false, Some("2f643d73")),
        format!("{app} (2f643d73)")
    );
    assert_eq!(client_version(false, Some("")), app);
    assert_eq!(client_version(false, None), app);
    assert_eq!(client_version(true, Some("2f643d73")), app);
}

#[test]
fn install_method_tells_source_builds_and_leaves_releases_unset() {
    let bundle = Path::new("/Users/me/herdr-gpui/target/release/Herdr.app/Contents/MacOS/Herdr");
    assert_eq!(
        install_method(bundle, false),
        Some("Locally built app bundle")
    );
    let cargo = Path::new("/Users/me/herdr-gpui/target/debug/sheprd");
    assert_eq!(install_method(cargo, false), Some("cargo run / just run"));
    let installed = Path::new("/usr/local/bin/herdr-gpui");
    assert_eq!(install_method(installed, false), None);
    let release = Path::new("/Applications/Herdr.app/Contents/MacOS/Herdr");
    assert_eq!(install_method(release, true), None);
    assert_eq!(install_method(cargo, true), None);
}

#[gpui::test]
fn herdr_window_report_issue_url_integrates_state(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);

    // Default window without error or daemon reports client version and OS
    let default_url = view.read_with(cx, |this, _| {
        this.report_issue().url("macOS, Apple Silicon", None)
    });
    assert!(default_url.contains("template=bug_report.yml"));
    assert!(default_url.contains("version="));
    assert!(default_url.contains("os=macOS%2C+Apple+Silicon"));
    assert!(!default_url.contains("install_method="));
    assert!(!default_url.contains("daemon_version="));
    assert!(!default_url.contains("logs="));

    // Connected daemon sets daemon_version
    view.update(cx, |this, _| {
        this.live.daemon_version = Some("0.9.5".into());
    });
    let with_daemon = view.read_with(cx, |this, _| {
        this.report_issue().url("macOS, Apple Silicon", None)
    });
    assert!(with_daemon.contains("daemon_version=0.9.5"));

    // Missing installation reports "none (not installed)" when version is absent
    view.update(cx, |this, _| {
        this.live.daemon_version = None;
        this.live.missing_installation = true;
    });
    let missing = view.read_with(cx, |this, _| {
        this.report_issue().url("macOS, Apple Silicon", None)
    });
    assert!(missing.contains("daemon_version=none+%28not+installed%29"));

    // An error populates logs
    view.update(cx, |this, _| {
        this.local_error = Some("Connection refused".into());
    });
    let with_error = view.read_with(cx, |this, _| {
        this.report_issue().url("macOS, Apple Silicon", None)
    });
    assert!(with_error.contains("logs="));
    assert!(with_error.contains("Connection+refused"));
}

#[gpui::test]
fn herdr_window_report_issue_keeps_the_error_seen_at_the_click(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |this, _| {
        this.live.daemon_version = Some("0.9.5".into());
        this.local_error = Some("Connection refused".into());
    });
    let report = view.read_with(cx, |this, _| this.report_issue());

    // A reconnect or host switch while the platform is gathered.
    view.update(cx, |this, _| {
        this.live.daemon_version = Some("0.10.0".into());
        this.local_error = None;
    });

    let url = report.url("macOS, Apple Silicon", None);
    assert!(url.contains("daemon_version=0.9.5"));
    assert!(url.contains("Connection+refused"));
}
