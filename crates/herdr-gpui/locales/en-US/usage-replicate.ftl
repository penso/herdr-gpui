# Replicate's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-replicate-setting-cookie = Sign in at https://replicate.com/account/billing, open Developer Tools > Application > Cookies > https://replicate.com, and copy sessionid (and csrftoken when present) as one "sessionid=value; csrftoken=value" header. An API token does not work here.
