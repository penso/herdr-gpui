//! Pairing a phone by QR code. The code encodes the web app's URL with the
//! token in the fragment (`#token=…`): browsers never send a fragment to the
//! server, so the token stays out of request lines, logs, and `Referer`.
//! Anyone who sees the code can use the companion, so it is shown only on
//! an interactive terminal.

use crate::Result;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use qrcode::{QrCode, render::unicode::Dense1x2};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

/// Tailscale's MagicDNS resolver. A route to it exists only on a tailnet,
/// and the local address on that route is the machine's tailnet address.
const TAILSCALE_PROBE: IpAddr = IpAddr::V4(Ipv4Addr::new(100, 100, 100, 100));
/// Any public address; the route to it leaves through the LAN interface.
const DEFAULT_ROUTE_PROBE: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));

/// An address a phone can open the web app at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneUrl {
    pub url: String,
    /// HTTPS, or a tailnet address whose traffic WireGuard encrypts. Plain
    /// HTTP elsewhere exposes the token to anyone on that network.
    pub encrypted: bool,
}

/// Where a phone can reach the server, best first: the explicit public URL,
/// then the tailnet address, then the LAN address. `local_ip_toward` answers
/// which local address the system would use to reach a host; [`route_probe`]
/// is the real one.
pub fn phone_urls(
    listen: SocketAddr,
    public_url: Option<&str>,
    local_ip_toward: impl Fn(IpAddr) -> Option<IpAddr>,
) -> Vec<PhoneUrl> {
    let mut urls = Vec::new();
    if let Some(url) = public_url {
        urls.push(PhoneUrl {
            url: url.trim_end_matches('/').to_owned(),
            encrypted: url.starts_with("https://"),
        });
    }
    let ip = listen.ip();
    let mut ips = Vec::new();
    if ip.is_unspecified() {
        // Every interface: the tailnet address first, if there is one.
        ips.extend(local_ip_toward(TAILSCALE_PROBE).filter(is_tailnet));
        ips.extend(local_ip_toward(DEFAULT_ROUTE_PROBE));
    } else if !ip.is_loopback() {
        ips.push(ip);
    }
    let mut seen = Vec::new();
    for ip in ips {
        if ip.is_loopback() || ip.is_unspecified() || seen.contains(&ip) {
            continue;
        }
        seen.push(ip);
        urls.push(PhoneUrl {
            url: format!("http://{}", SocketAddr::new(ip, listen.port())),
            encrypted: is_tailnet(&ip),
        });
    }
    urls
}

/// The local address the system would send from to reach `target`. A
/// connected UDP socket only consults the routing table; nothing is sent.
pub fn route_probe(target: IpAddr) -> Option<IpAddr> {
    let bind: SocketAddr = match target {
        IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        IpAddr::V6(_) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(bind).ok()?;
    socket.connect((target, 9)).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

/// Tailscale assigns addresses from the carrier-grade NAT range 100.64.0.0/10.
fn is_tailnet(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V4(v4) if v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
}

/// The link a phone opens to reach the web app already signed in.
pub fn pairing_url(base: &str, token: &str) -> String {
    let token = utf8_percent_encode(token, NON_ALPHANUMERIC);
    format!("{}/#token={token}", base.trim_end_matches('/'))
}

/// `text` as a QR code drawn with half-block characters, two modules per
/// character cell. Light modules are drawn as blocks, so the code reads
/// correctly on the dark background most terminals use.
pub fn render_qr(text: &str) -> Result<String> {
    let code = QrCode::new(text.as_bytes())?;
    Ok(code
        .render::<Dense1x2>()
        .dark_color(Dense1x2::Light)
        .light_color(Dense1x2::Dark)
        .quiet_zone(true)
        .build())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_token_rides_in_the_fragment() {
        assert_eq!(
            pairing_url("https://box.ts.net/", "abc123"),
            "https://box.ts.net/#token=abc123"
        );
        // Characters that would end or split the fragment are escaped.
        assert_eq!(
            pairing_url("http://h:1", "a b&c#d"),
            "http://h:1/#token=a%20b%26c%23d"
        );
    }

    #[test]
    fn renders_a_square_code_with_a_quiet_zone() {
        let qr = render_qr(&pairing_url("https://box.ts.net", &"f".repeat(64))).unwrap();
        let lines: Vec<&str> = qr.lines().collect();
        let width = lines[0].chars().count();
        assert!(lines.iter().all(|line| line.chars().count() == width));
        // Two modules per text row: the module grid is as tall as it is wide.
        assert!(
            (lines.len() * 2).abs_diff(width) <= 1,
            "{} rows for {width} columns",
            lines.len()
        );
        // The quiet zone is light, which this renderer draws as full blocks.
        assert!(lines[0].chars().all(|ch| ch == '█'));
    }

    fn lan_and_tailnet(target: IpAddr) -> Option<IpAddr> {
        Some(if target == TAILSCALE_PROBE {
            "100.101.102.103".parse().unwrap()
        } else {
            "192.168.1.20".parse().unwrap()
        })
    }

    #[test]
    fn all_interfaces_prefer_the_tailnet_then_the_lan() {
        let urls = phone_urls("0.0.0.0:8787".parse().unwrap(), None, lan_and_tailnet);
        assert_eq!(
            urls,
            [
                PhoneUrl {
                    url: "http://100.101.102.103:8787".into(),
                    encrypted: true
                },
                PhoneUrl {
                    url: "http://192.168.1.20:8787".into(),
                    encrypted: false
                },
            ]
        );
    }

    #[test]
    fn without_a_tailnet_the_lan_address_is_used() {
        // Off a tailnet, the probe toward 100.100.100.100 follows the
        // default route and answers with the LAN address.
        let lan_only = |_| "192.168.1.20".parse().ok();
        let urls = phone_urls("0.0.0.0:9000".parse().unwrap(), None, lan_only);
        assert_eq!(
            urls,
            [PhoneUrl {
                url: "http://192.168.1.20:9000".into(),
                encrypted: false
            }]
        );
        assert!(phone_urls("0.0.0.0:9000".parse().unwrap(), None, |_| None).is_empty());
    }

    #[test]
    fn a_public_url_comes_first_and_loopback_is_never_offered() {
        let urls = phone_urls(
            "127.0.0.1:8787".parse().unwrap(),
            Some("https://box.ts.net/"),
            lan_and_tailnet,
        );
        assert_eq!(
            urls,
            [PhoneUrl {
                url: "https://box.ts.net".into(),
                encrypted: true
            }]
        );
        assert!(phone_urls("127.0.0.1:8787".parse().unwrap(), None, lan_and_tailnet).is_empty());
        let urls = phone_urls("10.0.0.5:8787".parse().unwrap(), None, lan_and_tailnet);
        assert_eq!(
            urls,
            [PhoneUrl {
                url: "http://10.0.0.5:8787".into(),
                encrypted: false
            }]
        );
    }

    #[test]
    fn the_route_probe_finds_a_local_address_without_sending() {
        // Loopback always has a route, whatever the machine's network.
        assert_eq!(
            route_probe(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            Some(IpAddr::V4(Ipv4Addr::LOCALHOST))
        );
    }

    #[test]
    fn refuses_text_too_long_for_a_code() {
        assert!(matches!(
            render_qr(&"x".repeat(8000)),
            Err(crate::Error::Qr(_))
        ));
    }
}
