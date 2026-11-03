use super::Error;
use crate::browser::WebUrl;
use serde::Deserialize;
use std::{io::Read, path::Path, time::Duration};

const LIMIT: u64 = 8192;

/// The workspace path always refers to this machine. Accept only literal
/// loopback or localhost, and preserve a configured reverse-proxy base path.
pub(super) fn base(address: &WebUrl) -> Result<url::Url, Error> {
    let mut url = url::Url::parse(address.as_str()).map_err(|_| Error::Address)?;
    let local = match url.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if !local
        || url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::Address);
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

pub(super) fn probe(address: &WebUrl) -> Result<(), Error> {
    let url = base(address)?
        .join("api/wb/ping")
        .map_err(|_| Error::Address)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(3)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent.get(url.as_str()).call().map_err(Error::Http)?;
    if response.status().as_u16() != 200 {
        return Err(Error::NotWorkbench);
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Read)?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::NotWorkbench);
    }
    #[derive(Deserialize)]
    struct Ping {
        workbench: bool,
        version: String,
    }
    let ping: Ping = serde_json::from_slice(&bytes).map_err(Error::Json)?;
    if !ping.workbench || ping.version.is_empty() {
        return Err(Error::NotWorkbench);
    }
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct Discovery {
    pub(super) pid: u32,
    pub(super) url: WebUrl,
}

pub(super) fn discover(path: &Path) -> Result<Option<Discovery>, Error> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::Discovery(error)),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(Error::Discovery)?;
    if bytes.len() as u64 > LIMIT {
        return Ok(None);
    }
    let Ok(discovery) = serde_json::from_slice::<Discovery>(&bytes) else {
        return Ok(None);
    };
    if discovery.pid == 0 || base(&discovery.url).is_err() {
        return Ok(None);
    }
    Ok(Some(discovery))
}
