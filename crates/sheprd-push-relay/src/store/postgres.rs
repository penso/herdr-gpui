//! Revocations in Postgres. The connection comes back by itself: a request
//! that finds it closed opens a new one, and until then the relay answers
//! that it is unavailable, since a revocation may be in force.
//!
//! The address is secret, so it comes from a file, not an argument. The
//! connection is always encrypted, whatever the URL leaves out, unless it
//! says `sslmode=disable` for a database on this machine. A private CA goes
//! beside it.

use super::{Error, Result, Revoked, Store};
use crate::credential::DeviceId;
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, pem::PemObject},
};
use secrecy::{ExposeSecret, SecretString};
use std::{path::Path, str::FromStr, sync::Arc, time::Duration};
use tokio::sync::Mutex;
use tokio_postgres::{
    Client, Config,
    config::{Host, SslMode},
};
use tokio_postgres_rustls::MakeRustlsConnect;

/// How long reaching the database may take before the push is refused.
const CONNECT: Duration = Duration::from_secs(5);

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS revoked (
        id TEXT PRIMARY KEY NOT NULL,
        until_s BIGINT NOT NULL
    );
";

#[derive(Clone)]
pub struct Postgres {
    client: Arc<Mutex<Option<Arc<Client>>>>,
    config: Config,
    tls: MakeRustlsConnect,
}

/// Requires encryption in `config`: the driver's own default falls back to
/// plain text if the server does not offer it, which anyone on the path can
/// arrange. Plain text stays only when asked for, on this machine.
fn encrypted(mut config: Config) -> Result<Config> {
    match config.get_ssl_mode() {
        SslMode::Disable if config.get_hosts().iter().all(is_local) => {}
        SslMode::Disable => return Err(Error::PostgresPlainText),
        _ => {
            config.ssl_mode(SslMode::Require);
        }
    }
    Ok(config)
}

fn is_local(host: &Host) -> bool {
    match host {
        Host::Tcp(name) => {
            name == "localhost"
                || name
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        }
        #[cfg(unix)]
        Host::Unix(_) => true,
    }
}

/// Web PKI roots, and the private CA in `ca` when there is one.
fn tls(ca: Option<&Path>) -> Result<MakeRustlsConnect> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(path) = ca {
        let certificate =
            CertificateDer::from_pem_file(path).map_err(|source| Error::PostgresCa {
                path: path.to_owned(),
                source,
            })?;
        roots
            .add(certificate)
            .map_err(|source| Error::PostgresCaInvalid {
                path: path.to_owned(),
                source,
            })?;
    }
    Ok(MakeRustlsConnect::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

impl Postgres {
    /// Connects to `url`, trusting the private CA in `ca` as well as the
    /// usual roots, and creates the table if it is missing.
    pub async fn connect(url: &SecretString, ca: Option<&Path>) -> Result<Self> {
        let mut config = encrypted(Config::from_str(url.expose_secret())?)?;
        config.connect_timeout(CONNECT);
        let store = Self {
            client: Arc::default(),
            config,
            tls: tls(ca)?,
        };
        store.client().await?;
        Ok(store)
    }

    /// The open connection, or a new one when it closed. Requests share it;
    /// the lock is held only to look, or while reconnecting.
    async fn client(&self) -> Result<Arc<Client>> {
        let mut held = self.client.lock().await;
        if let Some(client) = held.as_ref().filter(|client| !client.is_closed()) {
            return Ok(client.clone());
        }
        let (client, connection) = self.config.connect(self.tls.clone()).await?;
        tokio::spawn(async move {
            if let Err(error) = connection.await {
                tracing::warn!(%error, "the Postgres connection closed");
            }
        });
        client.batch_execute(SCHEMA).await?;
        let client = Arc::new(client);
        *held = Some(client.clone());
        Ok(client)
    }
}

impl Store for Postgres {
    async fn revoke(&self, what: &Revoked, until_s: u64) -> Result<()> {
        let until_s = i64::try_from(until_s).unwrap_or(i64::MAX);
        self.client()
            .await?
            .execute(
                "INSERT INTO revoked (id, until_s) VALUES ($1, $2)
                 ON CONFLICT (id) DO UPDATE
                 SET until_s = GREATEST(revoked.until_s, EXCLUDED.until_s)",
                &[&what.key(), &until_s],
            )
            .await?;
        Ok(())
    }

    async fn is_revoked(&self, device: DeviceId, handle: &str) -> Result<bool> {
        let device = Revoked::Device(device).key();
        let handle = Revoked::Handle(handle.to_owned()).key();
        let row = self
            .client()
            .await?
            .query_one(
                "SELECT EXISTS (SELECT 1 FROM revoked WHERE id IN ($1, $2))",
                &[&device, &handle],
            )
            .await?;
        Ok(row.try_get(0)?)
    }

    async fn purge(&self, now_s: u64) -> Result<usize> {
        let now_s = i64::try_from(now_s).unwrap_or(i64::MAX);
        let dropped = self
            .client()
            .await?
            .execute("DELETE FROM revoked WHERE until_s <= $1", &[&now_s])
            .await?;
        Ok(usize::try_from(dropped).unwrap_or(usize::MAX))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
