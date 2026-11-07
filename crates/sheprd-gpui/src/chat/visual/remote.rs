//! A picture an agent linked on the web, fetched only once the user asks:
//! loading one tells its server this machine's address, as a page's
//! tracking pixel would. Only the host the user agreed to is asked, with no
//! redirect followed, and the answer is bounded and must be an image of a
//! type the chat decodes.

use super::{Drawn, Error, Result, decode_bytes};
use crate::browser::WebUrl;
use image::ImageFormat;
use std::time::Duration;

/// Bytes read from the server; a larger picture is refused.
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);

/// Fetches and decodes the picture at `url`. Blocks, so it runs on the
/// background executor.
pub(super) fn fetch(url: &WebUrl) -> Result<Drawn> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // The user agreed to ask this host, so a redirect to another, such
        // as one on this machine or its network, is not followed.
        .max_redirects(0)
        .build()
        .into();
    let bytes = agent
        .get(url.as_str())
        .call()
        .and_then(|mut response| {
            response
                .body_mut()
                .with_config()
                .limit(MAX_BYTES)
                .read_to_vec()
        })
        .map_err(|error| Error::Fetch(Box::new(error)))?;
    let format = image::guess_format(&bytes)
        .ok()
        .filter(|format| {
            matches!(
                format,
                ImageFormat::Gif | ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
            )
        })
        .ok_or(Error::NotAnImage)?;
    decode_bytes(bytes, format)
}
