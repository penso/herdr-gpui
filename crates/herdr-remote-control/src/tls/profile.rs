//! The iOS configuration profile that installs the CA.

use base64::{Engine, engine::general_purpose::STANDARD};
use std::fmt::Write;
use uuid::Uuid;

const DISPLAY_NAME: &str = "Herdr Remote";
const IDENTIFIER: &str = "so.pen.herdr-remote";
/// Wraps the base64 certificate like Apple's own profiles, so no line is
/// absurdly long.
const BASE64_LINE: usize = 52;

/// A profile with one `com.apple.security.root` payload carrying `ca_der`.
/// Its UUIDs come from `fingerprint`, so serving it twice gives the same
/// profile and iOS replaces rather than duplicates it.
pub(super) fn mobileconfig(ca_der: &[u8], fingerprint: &str) -> Vec<u8> {
    let profile_uuid = stable_uuid("profile", fingerprint);
    let payload_uuid = stable_uuid("root", fingerprint);
    let mut data = String::new();
    for (index, chunk) in STANDARD
        .encode(ca_der)
        .as_bytes()
        .chunks(BASE64_LINE)
        .enumerate()
    {
        if index > 0 {
            data.push('\n');
        }
        data.push_str("\t\t\t");
        // Base64 is ASCII, so every chunk is valid UTF-8.
        data.push_str(std::str::from_utf8(chunk).unwrap_or_default());
    }
    let mut xml = String::new();
    // Writing to a String cannot fail.
    let _ = write!(
        xml,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>PayloadContent</key>
	<array>
		<dict>
			<key>PayloadCertificateFileName</key>
			<string>herdr-remote-ca.cer</string>
			<key>PayloadContent</key>
			<data>
{data}
			</data>
			<key>PayloadDescription</key>
			<string>Lets this device trust the Herdr Remote server on your computer.</string>
			<key>PayloadDisplayName</key>
			<string>{DISPLAY_NAME} CA</string>
			<key>PayloadIdentifier</key>
			<string>{IDENTIFIER}.ca.{payload_uuid}</string>
			<key>PayloadType</key>
			<string>com.apple.security.root</string>
			<key>PayloadUUID</key>
			<string>{payload_uuid}</string>
			<key>PayloadVersion</key>
			<integer>1</integer>
		</dict>
	</array>
	<key>PayloadDescription</key>
	<string>Installs the certificate authority of Herdr Remote, which only vouches for private network addresses and local names.</string>
	<key>PayloadDisplayName</key>
	<string>{DISPLAY_NAME}</string>
	<key>PayloadIdentifier</key>
	<string>{IDENTIFIER}.{profile_uuid}</string>
	<key>PayloadRemovalDisallowed</key>
	<false/>
	<key>PayloadType</key>
	<string>Configuration</string>
	<key>PayloadUUID</key>
	<string>{profile_uuid}</string>
	<key>PayloadVersion</key>
	<integer>1</integer>
</dict>
</plist>
"#
    );
    xml.into_bytes()
}

/// An uppercase UUID derived from the CA fingerprint and a payload role.
fn stable_uuid(role: &str, fingerprint: &str) -> String {
    let name = format!("{IDENTIFIER}:{role}:{fingerprint}");
    Uuid::new_v5(&Uuid::NAMESPACE_OID, name.as_bytes())
        .hyphenated()
        .to_string()
        .to_ascii_uppercase()
}
