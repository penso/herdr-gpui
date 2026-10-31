//! What Settings fills in for this computer while sync is off: the short
//! name people call it by, and an address the other machines can reach it
//! at, its Tailscale name when it has one.

/// This computer's suggested place in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Suggestion {
    pub(crate) address: String,
    pub(crate) name: String,
}

impl Suggestion {
    /// A guess from the host name alone, which does not block.
    pub(crate) fn local() -> Self {
        let host = crate::remote::nearby::host_name();
        let name = short_name(&host).to_owned();
        Self {
            address: address(&user(), &lan_host(&name)),
            name,
        }
    }

    /// The local guess, addressed by this machine's Tailscale name when
    /// Tailscale is connected.
    /// Blocks on `tailscale status`: call it from the background executor.
    pub(super) fn find() -> Self {
        let mut suggestion = Self::local();
        match crate::menu::devices::own_tailscale_name() {
            Ok(Some(host)) => suggestion.address = address(&user(), &host),
            Ok(None) => {}
            Err(error) => {
                tracing::debug!(category = "machine_sync", %error, "Cannot read this machine's Tailscale name");
            }
        }
        suggestion
    }
}

fn user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// The first label of a host name: `MacBookPro` for
/// `MacBookPro.localdomain`.
pub(super) fn short_name(host: &str) -> &str {
    host.split('.')
        .next()
        .filter(|label| !label.is_empty())
        .unwrap_or(host)
}

/// The name other machines on the LAN reach `name` by: Bonjour's `.local`
/// on a Mac, which always answers it, and the bare name elsewhere.
fn lan_host(name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("{name}.local")
    } else {
        name.to_owned()
    }
}

/// `user@host`, or the bare host without a user.
pub(super) fn address(user: &str, host: &str) -> String {
    if user.is_empty() {
        host.to_owned()
    } else {
        format!("{user}@{host}")
    }
}
