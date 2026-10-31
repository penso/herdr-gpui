use super::*;

fn ssh(target: &str) -> Origin {
    Origin::of(&ConnectTarget::Ssh {
        target: target.into(),
        session: "default".into(),
    })
}

#[test]
fn local_targets_are_this_machine() {
    for target in [
        ConnectTarget::Local,
        ConnectTarget::Session {
            name: "work".into(),
            development: false,
        },
        ConnectTarget::Socket("/tmp/herdr.sock".into()),
    ] {
        assert_eq!(Origin::of(&target), Origin::Local, "{target:?}");
    }
    assert_eq!(
        Origin::of(&ConnectTarget::Wsl {
            distro: "Ubuntu".into(),
            session: "default".into(),
        }),
        Origin::Wsl
    );
}

#[test]
fn tailnet_names_and_addresses_are_tailscale() {
    for target in [
        "penso@100.84.59.116",
        "100.64.0.1",
        "100.127.255.255:22",
        "m4max.tail1234.ts.net",
        "user@M4MAX.TAIL1234.TS.NET.",
        "ssh://me@[fd7a:115c:a1e0::1]:22",
        "fd7a:115c:a1e0:ab12::1",
    ] {
        assert_eq!(ssh(target), Origin::Tailscale, "{target}");
    }
    // Neighbours of the ranges, and look-alike names, are plain SSH.
    for target in [
        "100.63.255.255",
        "100.128.0.1",
        "10.0.0.5",
        "fd7a:115c:a1e1::1",
        "ts.net.example.com",
        "build-box",
        "user@example.com",
    ] {
        assert_eq!(ssh(target), Origin::Ssh, "{target}");
    }
}

#[cfg(feature = "coder")]
#[test]
fn coder_config_ssh_hosts_and_saved_workspaces_are_coder() {
    use crate::cloud::CloudProvider;
    let over_ssh = Origin::Cloud {
        provider: CloudProvider::Coder,
        ssh: true,
    };
    assert_eq!(ssh("main.herdr-1.fabienpenso-microsoft.coder"), over_ssh);
    assert_eq!(ssh("me@workspace.owner.CODER"), over_ssh);
    assert_eq!(ssh("coder.example.com"), Origin::Ssh);
    assert_eq!(
        over_ssh.describe(),
        "\u{2068}Coder\u{2069} \u{2068}workspace\u{2069} over SSH"
    );
    assert_eq!(over_ssh.icon(), "icons/cloud/coder.svg");

    let saved = Origin::of(&ConnectTarget::Cloud {
        provider: CloudProvider::Coder,
        account: "https://coder.example.com".into(),
        id: "w1".into(),
        machine: "herdr-tidy-lark-aa".into(),
        session: "default".into(),
    });
    assert_eq!(
        saved,
        Origin::Cloud {
            provider: CloudProvider::Coder,
            ssh: false,
        }
    );
    assert_eq!(
        saved.describe(),
        "\u{2068}Coder\u{2069} \u{2068}workspace\u{2069}"
    );
}

#[cfg(feature = "azure")]
#[test]
fn azure_cloud_device_origin_and_icon() {
    use crate::cloud::CloudProvider;
    use gpui::AssetSource;
    let saved = Origin::of(&ConnectTarget::Cloud {
        provider: CloudProvider::Azure,
        account: "00000000-0000-0000-0000-000000000000".into(),
        id: "herdr-test-vm".into(),
        machine: "herdr-test-vm".into(),
        session: "default".into(),
    });
    assert_eq!(
        saved,
        Origin::Cloud {
            provider: CloudProvider::Azure,
            ssh: false,
        }
    );
    assert_eq!(
        saved.describe(),
        "\u{2068}Azure\u{2069} \u{2068}machine\u{2069}"
    );
    assert_eq!(saved.icon(), "icons/cloud/azure.svg");
    assert!(
        crate::icons::Icons
            .load(saved.icon())
            .ok()
            .flatten()
            .is_some()
    );
}

#[test]
fn every_mark_is_an_embedded_icon() {
    use gpui::AssetSource;
    for origin in [Origin::Local, Origin::Ssh, Origin::Tailscale, Origin::Wsl] {
        assert!(
            crate::icons::Icons
                .load(origin.icon())
                .ok()
                .flatten()
                .is_some(),
            "{origin:?}"
        );
        assert!(!origin.describe().is_empty());
    }
}

#[test]
fn host_names_drop_user_port_scheme_and_brackets() {
    assert_eq!(host_name(" ssh://Me@Host.Example.: "), "host.example");
    assert_eq!(host_name("me@host:2222"), "host");
    assert_eq!(host_name("[::1]:22"), "::1");
    assert_eq!(host_name("fe80::1"), "fe80::1");
}

/// Each provider's noun is a key the catalog selects a translated word by,
/// so every key needs its own variant rather than the fallback's `machine`.
#[cfg(feature = "cloud")]
#[test]
fn every_cloud_noun_has_its_own_translation() {
    for &provider in crate::cloud::CloudProvider::ALL {
        let noun = crate::cloud::noun(provider);
        let text = Origin::Cloud {
            provider,
            ssh: false,
        }
        .describe();
        assert!(
            text.ends_with(&format!(" \u{2068}{noun}\u{2069}")),
            "{provider:?}: {text}"
        );
    }
}
