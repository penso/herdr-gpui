//! Pairing metadata sampled on the blocking control worker. Origins remain
//! constrained by the certificate; adapter discovery never adds an origin.
use crate::control::{AddressDetails, AddressKind};
use std::{collections::BTreeMap, net::IpAddr};

pub(super) fn current(origins: &[String]) -> BTreeMap<String, AddressDetails> {
    let interfaces = if_addrs::get_if_addrs().unwrap_or_default();
    describe(
        origins,
        &interfaces
            .iter()
            .map(|interface| (interface.name.as_str(), interface.ip()))
            .collect::<Vec<_>>(),
    )
}

fn describe(origins: &[String], interfaces: &[(&str, IpAddr)]) -> BTreeMap<String, AddressDetails> {
    origins
        .iter()
        .map(|origin| {
            let url = url::Url::parse(origin).ok();
            let localhost = url
                .as_ref()
                .and_then(url::Url::host_str)
                .is_some_and(|host| host == "localhost" || host.ends_with(".localhost"));
            let ip = url.as_ref().and_then(|url| match url.host()? {
                url::Host::Ipv4(ip) => Some(IpAddr::V4(ip)),
                url::Host::Ipv6(ip) => Some(IpAddr::V6(ip)),
                url::Host::Domain(_) => None,
            });
            let mut names: Vec<String> = interfaces
                .iter()
                .filter(|(_, address)| Some(*address) == ip)
                .map(|(name, _)| name.chars().filter(|c| !c.is_control()).take(128).collect())
                .collect();
            names.sort();
            names.dedup();
            // CGNAT alone does not prove Tailscale. Its IPv6 prefix or an explicitly
            // named Tailscale adapter does, including the IPv4 on that adapter.
            let tailscale = ip.is_some_and(tailscale_v6)
                || interfaces.iter().any(|(name, address)| {
                    names.iter().any(|candidate| candidate == name)
                        && (tailscale_v6(*address)
                            || name.to_ascii_lowercase().contains("tailscale"))
                });
            let kind = if localhost || ip.is_some_and(|ip| ip.is_loopback()) {
                AddressKind::Loopback
            } else if tailscale {
                AddressKind::Tailscale
            } else if names.iter().any(|name| {
                name.starts_with("bridge") || name.starts_with("br-") || name == "docker0"
            }) {
                AddressKind::Bridge
            } else if ip.is_some_and(|ip| match ip {
                IpAddr::V4(ip) => ip.is_private(),
                IpAddr::V6(ip) => ip.is_unique_local(),
            }) {
                AddressKind::Local
            } else {
                AddressKind::Other
            };
            (
                origin.clone(),
                AddressDetails {
                    interfaces: names,
                    kind,
                },
            )
        })
        .collect()
}

fn tailscale_v6(ip: IpAddr) -> bool {
    matches!(ip, IpAddr::V6(ip) if ip.segments()[..3] == [0xfd7a, 0x115c, 0xa1e0])
}

#[cfg(test)]
mod tests;
