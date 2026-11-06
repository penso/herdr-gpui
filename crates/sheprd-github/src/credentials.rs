//! GitHub's own records in the private credential file: a token, or the
//! marker that the GitHub CLI's sign-in is used. Every read and write checks
//! the record decodes, which also bounds its size.

use super::{Result, token::Saved};
use secrecy::SecretString;
use std::{ffi::CStr, path::Path};

fn github_record(value: &SecretString) -> Result<()> {
    Saved::decode(value).map(drop)
}

pub(super) fn read(path: &Path, name: &CStr) -> Result<Option<SecretString>> {
    sheprd_credentials::file::read_checked(path, name, github_record)
}

/// Writes `token`, which needs `plaintext`, the user's opt-in, or removes
/// the record when it is `None`, which never does.
pub(super) fn store(
    path: &Path,
    name: &CStr,
    token: Option<&SecretString>,
    plaintext: bool,
) -> Result<()> {
    sheprd_credentials::file::store_checked(path, name, token, plaintext, github_record)
}

#[cfg(all(test, unix))]
mod tests;
