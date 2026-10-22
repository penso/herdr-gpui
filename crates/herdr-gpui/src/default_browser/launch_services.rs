//! The default browser from Launch Services: the application macOS opens an
//! `https` URL with, started through its bundle's executable.

use super::{Error, Result, spawn};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSString, NSURL};

/// Safari ignores a URL given on its command line.
const SAFARI: &str = "com.apple.Safari";
const PROBE: &str = "https://example.com";

pub(super) fn open(url: &str) -> Result<()> {
    let probe = NSURL::URLWithString(&NSString::from_str(PROBE)).ok_or(Error::NoDefaultBrowser)?;
    let app = NSWorkspace::sharedWorkspace()
        .URLForApplicationToOpenURL(&probe)
        .ok_or(Error::NoDefaultBrowser)?;
    let app_path = app.to_file_path().ok_or(Error::NoDefaultBrowser)?;
    let unstartable = || Error::BrowserBundle {
        app: app_path.clone(),
    };
    let bundle = NSBundle::bundleWithURL(&app).ok_or_else(unstartable)?;
    if bundle
        .bundleIdentifier()
        .is_some_and(|id| id.to_string() == SAFARI)
    {
        return Err(Error::Unsupported);
    }
    let program = bundle
        .executableURL()
        .and_then(|executable| executable.to_file_path())
        .ok_or_else(unstartable)?;
    spawn(&program, &[url.to_owned()])
}
