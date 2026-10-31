# Failures reading or saving a sign-in (GitHub, a cloud provider) in the
# system keyring or the private credential file. Keep config keys and
# values (allow_plaintext_credentials = true, [github], [coder]) and the
# environment variables GH_TOKEN and GITHUB_TOKEN as they are.

error-credentials-encoding = Invalid GitHub credential encoding.
error-credentials-io = Cannot access private credential file.
# macOS only.
error-credentials-keychain-read = Cannot read GitHub Keychain entry. Unlock your login Keychain or set GH_TOKEN.
# macOS only.
error-credentials-keychain-write = GitHub Keychain update failed. Unlock your login Keychain and try again.
# 0600 is the file's Unix permissions: only its owner can read and write it.
error-credentials-permissions = Cannot access private credential file. Require an owned directory and regular 0600 file; symlinks are rejected.
error-credentials-policy = No secure credential store configured. Explicitly opt in with allow_plaintext_credentials = true in the [github] or [coder] table, or use GH_TOKEN / GITHUB_TOKEN for GitHub.
# Linux only.
error-credentials-secret-service-read = Cannot read GitHub sign-in from the desktop keyring. Unlock your keyring or set GH_TOKEN.
# Linux only. Secret Service is the desktop keyring's protocol; GNOME
# Keyring, KWallet, and KeePassXC are programs that provide it.
error-credentials-secret-service-unavailable = No desktop keyring (Secret Service) is running. Start GNOME Keyring, KWallet, or KeePassXC, or set [github] allow_plaintext_credentials = true, or use GH_TOKEN / GITHUB_TOKEN.
# Linux only.
error-credentials-secret-service-write = GitHub keyring update failed. Unlock your desktop keyring and try again.
error-credentials-unsupported = No GitHub credential store is available on this platform. Use GH_TOKEN / GITHUB_TOKEN.

# Windows only.
error-credentials-credential-manager-read = Cannot read a saved sign-in from Windows Credential Manager. Sign in again, or set GH_TOKEN for GitHub.
# Windows only.
error-credentials-credential-manager-write = Windows Credential Manager update failed. Try again.
