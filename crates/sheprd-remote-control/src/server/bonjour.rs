//! Bonjour: advertises `_herdr-remote._tcp` so the iOS app finds this
//! computer on the local network, to pair with it nearby and to reach it
//! again after its address changes (`docs/ios-app-plan.md`). The TXT
//! record says whether a pairing dialog is open, so a phone offers to pair
//! only then. Advertising is a convenience: a failure is logged and the
//! daemon carries on.

use crate::{auth::nearby::fingerprint_bytes, web::State};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::{sync::Arc, time::Duration};
use tokio::sync::watch;

pub(super) const SERVICE: &str = "_herdr-remote._tcp.local.";
/// How often the pairing dialog's state is checked for the TXT record.
const CHECK: Duration = Duration::from_secs(1);

/// The TXT record: the format's version, the fingerprint's first 16 hex
/// digits (enough to tell paired computers apart, not to pin one), and
/// whether a pairing dialog is open.
pub(super) fn txt(fingerprint: &str, pairing: bool) -> Vec<(&'static str, String)> {
    let prefix: String = fingerprint_bytes(fingerprint)
        .map(|bytes| {
            bytes[..8]
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect()
        })
        .unwrap_or_default();
    vec![
        ("v", "1".to_owned()),
        ("fp", prefix),
        ("pairing", if pairing { "1" } else { "0" }.to_owned()),
    ]
}

/// A host name mDNS accepts for `computer`: letters, digits, and hyphens.
pub(super) fn host_name(computer: &str) -> String {
    let label: String = computer
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .chars()
        .take(63)
        .collect();
    let label = if label.is_empty() {
        "herdr".to_owned()
    } else {
        label
    };
    format!("{label}.local.")
}

/// Advertises until `stopped`, updating the record when the pairing
/// dialog opens or closes.
pub(super) async fn advertise(state: Arc<State>, mut stopped: watch::Receiver<bool>) {
    let daemon = match ServiceDaemon::new() {
        Ok(daemon) => daemon,
        Err(error) => {
            tracing::warn!(%error, "Bonjour is unavailable; phones pair by QR code");
            return;
        }
    };
    let mut advertised = None;
    let mut fullname = None;
    let mut ticker = tokio::time::interval(CHECK);
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            _ = stopped.wait_for(|stop| *stop) => break,
        }
        let pairing = state.auth().pairing_open(crate::web::now_ms());
        if advertised == Some(pairing) {
            continue;
        }
        let properties = txt(&state.fingerprint, pairing);
        let pairs: Vec<(&str, &str)> = properties
            .iter()
            .map(|(key, value)| (*key, value.as_str()))
            .collect();
        let info = ServiceInfo::new(
            SERVICE,
            &state.computer,
            &host_name(&state.computer),
            (),
            state.port,
            pairs.as_slice(),
        )
        .map(ServiceInfo::enable_addr_auto);
        match info.and_then(|info| {
            let name = info.get_fullname().to_owned();
            daemon.register(info).map(|()| name)
        }) {
            Ok(name) => {
                advertised = Some(pairing);
                fullname = Some(name);
            }
            Err(error) => {
                tracing::warn!(%error, "Could not advertise over Bonjour");
                // Not retried every second: the next dialog change tries again.
                advertised = Some(pairing);
            }
        }
    }
    if let Some(name) = fullname {
        let _ = daemon.unregister(&name);
    }
    let _ = daemon.shutdown();
}
