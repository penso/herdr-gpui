//! The default browser's command from the Windows registry: the ProgId the
//! user chose for `https` (else `http`) links, and that ProgId's
//! `shell\open\command`.

use super::{Error, Result, command_line, spawn};
use windows_registry::{CLASSES_ROOT, CURRENT_USER, Key};
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;

const ASSOCIATIONS: &str = r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations";

pub(super) fn open(url: &str) -> Result<()> {
    let prog_id = prog_id()?;
    let key = format!(r"{prog_id}\shell\open\command");
    let template = read(CLASSES_ROOT, &key, "")?.ok_or(Error::NoDefaultBrowser)?;
    let (program, args) = command_line(&template, url)?;
    spawn(&program, &args)
}

/// Where Windows records the user's choice for a scheme, newest first:
/// Windows 11 writes `UserChoiceLatest\ProgId`, and may leave the older
/// `UserChoice` key out.
const CHOICES: [&str; 2] = [r"UserChoiceLatest\ProgId", "UserChoice"];

/// The ProgId the user chose for `https`, else `http`; a missing key or
/// value means that scheme has no choice recorded there.
fn prog_id() -> Result<String> {
    for scheme in ["https", "http"] {
        for choice in CHOICES {
            let key = format!(r"{ASSOCIATIONS}\{scheme}\{choice}");
            if let Some(prog_id) = read(CURRENT_USER, &key, "ProgId")?.filter(|id| !id.is_empty()) {
                return Ok(prog_id);
            }
        }
    }
    Err(Error::NoDefaultBrowser)
}

/// The string `value` of `root\key` (`""` is the key's default value), or
/// `None` when the key or the value does not exist.
fn read(root: &Key, key: &str, value: &str) -> Result<Option<String>> {
    match root.open(key).and_then(|opened| opened.get_string(value)) {
        Ok(text) => Ok(Some(text)),
        Err(error)
            if windows_result::WIN32_ERROR::from_error(&error)
                .is_some_and(|code| code.0 == ERROR_FILE_NOT_FOUND) =>
        {
            Ok(None)
        }
        Err(source) => Err(Error::Registry {
            key: key.to_owned(),
            source,
        }),
    }
}
