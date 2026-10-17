use super::*;
use std::path::Path;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod posix_exec;
mod powershell_exec;
mod scripts;

/// A file name every shell would reinterpret if it were not quoted: spaces,
/// both kinds of single quote, `$`, a backtick, non-ASCII, and a leading `-`.
const HOSTILE_NAME: &str = "-a b'c $x `y \u{e9}\u{2019}.bin";

/// Bytes a text-mode stream would mangle, then a deterministic noise tail.
fn payload(len: usize) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x0A, 0x0D, 0xFF, 0x0D, 0x0A, 0x1A, 0x00];
    let mut state: u32 = 0x1234_5678;
    while bytes.len() < len {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        bytes.push(state.to_be_bytes()[0]);
    }
    bytes.truncate(len);
    bytes
}

/// The script failure inside `error`, if that is what it is.
fn script_error(error: &Error) -> Option<&crate::Error> {
    match error {
        Error::Script(inner) => Some(inner.as_ref()),
        _ => None,
    }
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The names in `dir`, sorted, so a leftover temporary file shows up.
fn entries(dir: &Path) -> Vec<String> {
    let mut names = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

/// `script` as `-EncodedCommand` expects it: UTF-16LE, then base64.
fn encode(script: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let utf16 = script
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    STANDARD.encode(utf16)
}

fn decode(encoded: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let bytes = STANDARD.decode(encoded).unwrap();
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).unwrap()
}
