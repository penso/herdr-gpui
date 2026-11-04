//! Windows' own Authenticode check of a downloaded installer, and its signer.
//!
//! `WinVerifyTrust` is the only way to have Windows verify a PE signature and
//! its certificate chain, and neither the standard library nor a maintained
//! safe crate wraps it, so this module calls it directly. Every pointer handed
//! over refers to locals that outlive the call, and the state Windows
//! allocates for the signer is read before the same call releases it.
#![allow(unsafe_code)]
use super::Error;
use std::{fs::File, os::windows::io::AsRawHandle, path::Path, ptr};
use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE},
    Security::{
        Cryptography::{
            CERT_NAME_STR_REVERSE_FLAG, CERT_X500_NAME_STR, CertNameToStrW, X509_ASN_ENCODING,
        },
        WinTrust::{
            WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
            WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE,
            WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvSignerFromChain,
            WTHelperProvDataFromStateData, WinVerifyTrust,
        },
    },
};

/// Requires a valid Authenticode signature on `file`, which `path` names,
/// whose signing certificate's subject is exactly `subject`, in the form
/// PowerShell's `Get-AuthenticodeSignature` prints it ("CN=…, O=…, C=…").
///
/// Revocation is not checked: that needs the network at an arbitrary moment,
/// and this signature is the second check, after the file's SHA-256 matched
/// the release's Ed25519-signed manifest.
pub(super) fn verify(file: &File, path: &Path, subject: &str) -> Result<(), Error> {
    let signer = signer(file, path)?;
    if signer != subject {
        return Err(Error::Signer { found: signer });
    }
    Ok(())
}

/// The subject of the certificate that signed `file`, once Windows has
/// verified the signature and its chain to a trusted root.
fn signer(file: &File, path: &Path) -> Result<String, Error> {
    let wide = wide_nul(path).ok_or(Error::InstallerPath)?;
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: wide.as_ptr(),
        // Windows reads the open handle, so the bytes checked are the ones
        // this process holds open and will run, not whatever the path names.
        hFile: file.as_raw_handle() as HANDLE,
        pgKnownSubject: ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
        ..Default::default()
    };
    data.Anonymous.pFile = &mut info;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `data` points at `info`, which points at `wide` and `file`'s
    // handle; all four live until the close call below. INVALID_HANDLE_VALUE
    // as the window means no UI may be shown.
    let status = unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE as _,
            &mut action,
            (&raw mut data).cast(),
        )
    };
    let subject = if status == 0 {
        // SAFETY: a successful VERIFY leaves `hWVTStateData` holding provider
        // data until the CLOSE below; each pointer read here is checked for
        // null and only read, never kept past this block.
        unsafe { subject(data.hWVTStateData) }
    } else {
        Err(Error::Untrusted(status))
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: as above; CLOSE frees the state the VERIFY call allocated.
    unsafe {
        WinVerifyTrust(
            INVALID_HANDLE_VALUE as _,
            &mut action,
            (&raw mut data).cast(),
        );
    }
    subject
}

/// # Safety
/// `state` must be the `hWVTStateData` of a successful, not yet closed,
/// `WTD_STATEACTION_VERIFY` call.
unsafe fn subject(state: HANDLE) -> Result<String, Error> {
    unsafe {
        let provider = WTHelperProvDataFromStateData(state);
        if provider.is_null() {
            return Err(Error::MissingSigner);
        }
        let signer = WTHelperGetProvSignerFromChain(provider, 0, 0, 0);
        if signer.is_null() || (*signer).csCertChain == 0 || (*signer).pasCertChain.is_null() {
            return Err(Error::MissingSigner);
        }
        // The chain starts with the signing certificate itself.
        let certificate = (*(*signer).pasCertChain).pCert;
        if certificate.is_null() || (*certificate).pCertInfo.is_null() {
            return Err(Error::MissingSigner);
        }
        let name = &raw const (*(*certificate).pCertInfo).Subject;
        let flags = CERT_X500_NAME_STR | CERT_NAME_STR_REVERSE_FLAG;
        let length = CertNameToStrW(X509_ASN_ENCODING, name, flags, ptr::null_mut(), 0);
        if length <= 1 {
            return Err(Error::MissingSigner);
        }
        let mut buffer = vec![0u16; length as usize];
        let written = CertNameToStrW(X509_ASN_ENCODING, name, flags, buffer.as_mut_ptr(), length);
        // The count includes the terminating null.
        buffer.truncate(written.saturating_sub(1) as usize);
        String::from_utf16(&buffer).map_err(|_| Error::MissingSigner)
    }
}

/// The UTF-16 path with a terminating null, or `None` if it already holds
/// one, which Windows would read as the end of the path.
fn wide_nul(path: &Path) -> Option<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return None;
    }
    wide.push(0);
    Some(wide)
}
