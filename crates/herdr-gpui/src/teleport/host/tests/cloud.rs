use super::*;
use herdr_client::CloudProvider;

#[test]
fn cloud_hosts_preserve_provider_account_machine_and_session() {
    for &provider in CloudProvider::ALL {
        let target = ConnectTarget::Cloud {
            provider,
            account: "account".into(),
            id: "machine".into(),
            machine: "Display name".into(),
            session: "work's session".into(),
        };
        let host = crate::teleport::host_for(&target).unwrap();
        assert!(
            host.prelude()
                .contains("herdr --session 'work'\\''s session'")
        );
        for (account, id, session) in [
            ("other", "machine", "work's session"),
            ("account", "other", "work's session"),
            ("account", "machine", "other"),
        ] {
            let changed = ConnectTarget::Cloud {
                provider,
                account: account.into(),
                id: id.into(),
                machine: "Renamed display name".into(),
                session: session.into(),
            };
            assert_ne!(host, crate::teleport::host_for(&changed).unwrap());
        }
        // Cancellation must short circuit provider configuration and network work.
        let error = host.capture("exit 99", &AtomicBool::new(true)).unwrap_err();
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            assert!(matches!(error, herdr_client::Error::ScriptCancelled));
        } else {
            assert!(matches!(error, herdr_client::Error::ScriptUnsupported));
        }
    }
}
