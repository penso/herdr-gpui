# Kimi Code's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-kimi-setting-api-key = A Kimi Code API key from the Kimi Code console (https://www.kimi.com/code/console, or https://www.kimi.ai/code/console for International). Not needed when the Kimi Code CLI is signed in on the host.
usage-kimi-setting-region = Which Kimi service the credentials belong to: "china" (the default, kimi.com) or "international" (kimi.ai). Moonshot Open Platform keys belong to the moonshot provider instead.
usage-kimi-setting-base-url = An https Kimi Code API base to use with the API key instead of https://api.kimi.com, for a compatible proxy. Setting it stops the CLI's sign-in from being used.
usage-kimi-setting-token = Your Kimi web session, for the membership pool and plan, or on its own. Sign in at https://www.kimi.com/code/console, open Developer Tools → Application → Cookies → https://www.kimi.com, and copy the value of the `kimi-auth` cookie (a JWT starting with eyJ), without its name. On kimi.ai, copy `access_token` from Local Storage instead. Not needed when Kimi is listed in show_providers and you are signed in with Chrome or Safari.
usage-kimi-setting-cookie = Instead of token, a whole Kimi Cookie header that includes kimi-auth, as "kimi-auth=value; …".
