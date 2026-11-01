# Windsurf's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-windsurf-setting-session = The windsurf.com session bundle, for live quota. Sign in at https://windsurf.com/profile in Chrome, open Developer Tools → Application → Local Storage → https://windsurf.com (or https://app.devin.ai), and copy the values of devin_session_token, devin_auth1_token, devin_account_id, and devin_primary_org_id into one JSON object: {"{"}"devin_session_token": "…", "devin_auth1_token": "…", "devin_account_id": "…", "devin_primary_org_id": "…"{"}"}. Without it, the plan the Windsurf editor cached at its last launch is shown; on the Mac running Herdr only when windsurf is in show_providers, as macOS may ask before Herdr reads another app's data.
