//! What this machine is reachable as, and the private ranges the CA is
//! constrained to.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The address ranges the CA may vouch for, as `(network, prefix length)`.
/// A leaf only ever carries addresses inside them.
pub(super) const PRIVATE_NETWORKS: &[(IpAddr, u8)] = &[
    (IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)), 8),
    (IpAddr::V4(Ipv4Addr::new(172, 16, 0, 0)), 12),
    (IpAddr::V4(Ipv4Addr::new(192, 168, 0, 0)), 16),
    // Carrier-grade NAT, where Tailscale hands out its 100.x addresses.
    (TAILSCALE_V4.0, TAILSCALE_V4.1),
    (IpAddr::V4(Ipv4Addr::new(169, 254, 0, 0)), 16),
    (IpAddr::V4(Ipv4Addr::new(127, 0, 0, 0)), 8),
    // Unique local addresses; Tailscale's fd7a:115c:a1e0::/48 is inside.
    (IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 0)), 8),
    (LINK_LOCAL_V6.0, LINK_LOCAL_V6.1),
    (IpAddr::V6(Ipv6Addr::LOCALHOST), 128),
];

/// The DNS subtrees the CA may vouch for: each name and its subdomains.
/// Not `ts.net`: a stolen CA key would vouch for every tailnet, and
/// Tailscale is reached by its address anyway.
pub(super) const LOCAL_DOMAINS: &[&str] = &["local", "localhost"];

const TAILSCALE_V4: (IpAddr, u8) = (IpAddr::V4(Ipv4Addr::new(100, 64, 0, 0)), 10);
const TAILSCALE_V6: (IpAddr, u8) = (
    IpAddr::V6(Ipv6Addr::new(0xfd7a, 0x115c, 0xa1e0, 0, 0, 0, 0, 0)),
    48,
);
/// Link-local IPv6 needs a zone index no URL can carry, so it is permitted
/// by the CA but never listed in [`Names`].
const LINK_LOCAL_V6: (IpAddr, u8) = (IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 0)), 10);
const TAILSCALE_DOMAIN: &str = ".ts.net";

/// The names a server certificate lists. Sorted and deduplicated, so two
/// lists of the same names compare equal and a refresh can tell nothing
/// changed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Names {
    pub(crate) dns: Vec<String>,
    pub(crate) ips: Vec<IpAddr>,
}

impl Names {
    /// This machine's names now: its interface addresses inside
    /// [`PRIVATE_NETWORKS`], loopback, `localhost`, and `HOSTNAME.local`.
    /// Blocking but quick; an interface listing failure leaves loopback.
    pub(crate) fn current() -> Self {
        let addresses: Vec<IpAddr> = if_addrs::get_if_addrs()
            .map(|interfaces| interfaces.iter().map(if_addrs::Interface::ip).collect())
            .unwrap_or_default();
        Self::new(addresses, host_name().as_deref())
    }

    /// Names from `addresses` and `host`: addresses outside the private
    /// ranges and link-local IPv6 are dropped, loopback and `localhost` are
    /// always added, and `host` (a host name, `.local` or a domain allowed)
    /// becomes `LABEL.local` when its first label is a valid DNS label.
    pub(crate) fn new(addresses: impl IntoIterator<Item = IpAddr>, host: Option<&str>) -> Self {
        let mut ips: Vec<IpAddr> = addresses
            .into_iter()
            .filter(|ip| is_private(*ip) && !within(*ip, LINK_LOCAL_V6))
            .chain([
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(Ipv6Addr::LOCALHOST),
            ])
            .collect();
        ips.sort_unstable();
        ips.dedup();
        let mut names = Self {
            dns: vec!["localhost".to_owned()],
            ips,
        };
        if let Some(label) = host.and_then(mdns_label) {
            names.add_dns(format!("{label}.local"));
        }
        names.normalize();
        names
    }

    /// The names a user-supplied leaf certificate lists, as they are: its
    /// own domain is what phones reach it by. Wildcards are left out, as
    /// no origin is one; a certificate that cannot be read lists nothing.
    pub(crate) fn from_certificate(der: &[u8]) -> Self {
        use x509_parser::extensions::GeneralName;
        let mut names = Self::default();
        let Ok((_, certificate)) = x509_parser::parse_x509_certificate(der) else {
            return names;
        };
        let Ok(Some(alternatives)) = certificate.subject_alternative_name() else {
            return names;
        };
        for name in &alternatives.value.general_names {
            match name {
                GeneralName::DNSName(dns) if !dns.contains('*') => {
                    names
                        .dns
                        .push(dns.trim_end_matches('.').to_ascii_lowercase());
                }
                GeneralName::IPAddress(bytes) => {
                    let ip = match bytes.len() {
                        4 => <[u8; 4]>::try_from(*bytes).map(IpAddr::from).ok(),
                        16 => <[u8; 16]>::try_from(*bytes).map(IpAddr::from).ok(),
                        _ => None,
                    };
                    names.ips.extend(ip);
                }
                _ => {}
            }
        }
        names.ips.sort_unstable();
        names.ips.dedup();
        names.normalize();
        names
    }

    /// Only the names a server listening on `bind` answers at: all of them
    /// when it listens everywhere, else that address alone, plus the user's
    /// own domains (`localhost` too when `bind` is loopback). `.local` names
    /// resolve to every interface, so a single address drops them.
    pub(crate) fn bound_to(mut self, bind: IpAddr) -> Self {
        if bind.is_unspecified() {
            return self;
        }
        self.ips.retain(|ip| *ip == bind);
        self.dns.retain(|name| {
            if is_local_domain(name) {
                bind.is_loopback() && reach_of_dns(name) == Reach::Loopback
            } else {
                true
            }
        });
        self
    }

    /// Adds a DNS name, such as the Tailscale MagicDNS name, keeping the
    /// list sorted. A name outside [`LOCAL_DOMAINS`] would make the leaf
    /// fail verification, so it is ignored.
    pub(crate) fn add_dns(&mut self, name: impl Into<String>) {
        let name = name.into().trim_end_matches('.').to_ascii_lowercase();
        if !is_local_domain(&name) {
            return;
        }
        self.dns.push(name);
        self.normalize();
    }

    /// `https://` origins for every name on `port`, the likeliest to answer
    /// from a phone first: LAN IPv4, then Tailscale, then other addresses
    /// and `.local` names, then loopback. IPv6 addresses are bracketed.
    pub(crate) fn origins(&self, port: u16) -> Vec<String> {
        let mut ranked: Vec<(Reach, String)> = self
            .ips
            .iter()
            .map(|ip| {
                let host = match ip {
                    IpAddr::V4(v4) => v4.to_string(),
                    IpAddr::V6(v6) => format!("[{v6}]"),
                };
                (reach_of_ip(*ip), host)
            })
            .chain(
                self.dns
                    .iter()
                    .map(|name| (reach_of_dns(name), name.clone())),
            )
            .collect();
        // Stable, so each group keeps the sorted order.
        ranked.sort_by_key(|(reach, _)| *reach);
        // Written as browsers send `Origin`, which leaves out HTTPS's own
        // port, so exact comparison accepts them.
        let port = if port == 443 {
            String::new()
        } else {
            format!(":{port}")
        };
        ranked
            .into_iter()
            .map(|(_, host)| format!("https://{host}{port}"))
            .collect()
    }

    fn normalize(&mut self) {
        self.dns.sort_unstable();
        self.dns.dedup();
    }
}

/// How likely a phone is to reach an address, best first.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Reach {
    Lan,
    Tailscale,
    Other,
    Loopback,
}

fn reach_of_ip(ip: IpAddr) -> Reach {
    if ip.is_loopback() {
        return Reach::Loopback;
    }
    if within(ip, TAILSCALE_V4) || within(ip, TAILSCALE_V6) {
        return Reach::Tailscale;
    }
    match ip {
        IpAddr::V4(v4) if v4.is_private() => Reach::Lan,
        _ => Reach::Other,
    }
}

fn reach_of_dns(name: &str) -> Reach {
    if name == "localhost" || name.ends_with(".localhost") {
        return Reach::Loopback;
    }
    if name.ends_with(TAILSCALE_DOMAIN) {
        return Reach::Tailscale;
    }
    Reach::Other
}

/// Whether `ip` is inside one of [`PRIVATE_NETWORKS`].
pub(super) fn is_private(ip: IpAddr) -> bool {
    PRIVATE_NETWORKS.iter().any(|network| within(ip, *network))
}

/// Whether `name` is one of [`LOCAL_DOMAINS`] or below one.
fn is_local_domain(name: &str) -> bool {
    LOCAL_DOMAINS.iter().any(|domain| {
        name == *domain
            || name
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.'))
    })
}

fn within(ip: IpAddr, (network, prefix): (IpAddr, u8)) -> bool {
    match (ip, network) {
        (IpAddr::V4(ip), IpAddr::V4(network)) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
            u32::from(ip) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(network)) => {
            let mask = u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0);
            u128::from(ip) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

/// The lowercase first label of `host` when it is a valid DNS label, the
/// name mDNS answers for as `LABEL.local`.
pub(super) fn mdns_label(host: &str) -> Option<String> {
    let label = host.split('.').next()?.to_ascii_lowercase();
    let valid = !label.is_empty()
        && label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-');
    valid.then_some(label)
}

/// This machine's host name, for `HOSTNAME.local` and the CA's name.
pub(super) fn host_name() -> Option<String> {
    #[cfg(unix)]
    {
        let uname = rustix::system::uname();
        let name = uname.nodename().to_str().ok()?.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }
    #[cfg(windows)]
    {
        let name = std::env::var("COMPUTERNAME").ok()?;
        let name = name.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }
    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}
