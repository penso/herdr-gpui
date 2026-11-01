# OpenCode Go's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-opencodego-setting-api-key = An OpenCode API key for the account with the Go subscription, created at https://opencode.ai/auth under API Keys. Preferred over the cookie. When unset, the key `opencode auth login` saved in ~/.local/share/opencode/auth.json is used.
usage-opencodego-setting-cookie = Your opencode.ai session, used when no API key is set. Sign in at https://opencode.ai/auth, open Developer Tools > Application > Cookies > https://opencode.ai, and copy the __Host-console_session and auth cookies (either one alone also works). Paste them as "__Host-console_session=value; auth=value".
usage-opencodego-setting-workspace-id = Optional, for the cookie sign-in. The workspace to read, as a wrk_... or org_... ID or its https://opencode.ai/console/... URL. Defaults to the first workspace of the account.

## The report: the Go subscription's windows and the workspace's Zen
## balance. Go and OpenCode Go are the product's names; Zen is OpenCode's
## model gateway.

usage-opencodego-zen-balance = Zen balance
usage-opencodego-subscription = Subscription
# The workspace has no OpenCode Go subscription.
usage-opencodego-none = None
