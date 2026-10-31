//! Two shell round trips, preserving the before/after snapshot fence. Each
//! payload group keeps the former separate query's one-MiB output limit.

use super::*;

pub(super) fn query(host: &Host, script: &str, cancelled: &AtomicBool) -> Result<Vec<u8>, Error> {
    let mut output = Vec::new();
    let limits = ScriptLimits {
        output: LIMITS.output * 2 + 16 * 1024,
        ..LIMITS
    };
    host.stream(script, std::io::empty(), &mut output, limits, cancelled)
        .map_err(Error::Query)?;
    Ok(output)
}

pub(super) fn section(script: &mut String, name: &str) {
    script.push_str(&format!("printf '\\036%s\\n' {}\n", shell_quote(name)));
}

pub(super) fn snapshot(script: &mut String) {
    section(script, "snapshot");
    script.push_str("herdr_cli api snapshot\n");
}

pub(super) fn finish(script: &mut String) {
    section(script, "end");
}

/// Exact ordered framing rejects truncated, extra, duplicate, or injected
/// sections instead of mistaking them for another pane's metadata.
pub(super) fn frames<'a>(bytes: &'a [u8], names: &[String]) -> Result<Vec<&'a str>, Error> {
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Framing)?;
    let mut parts = text.split('\u{1e}');
    if parts.next() != Some("") {
        return Err(Error::Framing);
    }
    let mut result = Vec::with_capacity(names.len());
    let mut metadata = 0;
    for name in names {
        let (found, payload) = parts
            .next()
            .and_then(|part| part.split_once('\n'))
            .ok_or(Error::Framing)?;
        if found != name || payload.len() as u64 > LIMITS.output {
            return Err(Error::Framing);
        }
        if name != "snapshot" {
            metadata += (payload.len() + found.len() + 2) as u64;
            if metadata > LIMITS.output {
                return Err(Error::Framing);
            }
        }
        result.push(payload);
    }
    if parts.next() != Some("end\n") || parts.next().is_some() {
        return Err(Error::Framing);
    }
    Ok(result)
}

pub(super) fn decode_snapshot(json: &str) -> Result<SnapshotResult, Error> {
    serde_json::from_str::<Envelope<SnapshotResult>>(json)
        .map(|answer| answer.result)
        .map_err(Error::Answer)
}
