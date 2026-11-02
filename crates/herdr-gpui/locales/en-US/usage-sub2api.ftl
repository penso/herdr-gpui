# sub2api's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-sub2api-setting-api-key = A sub2api group API key (sk-…), created in your sub2api deployment's dashboard under API keys. Usage is scoped to that key's group.
usage-sub2api-setting-base-url = Your sub2api deployment, e.g. https://sub2api.example.com. HTTPS is required, except plain HTTP on a loopback address such as http://127.0.0.1:8080.

## The report: the key's quota, rate limits, request and token totals, and
## when it expires. Plan names stay as the Sub2API instance sends them.

usage-sub2api-quota = Quota
# A rate limit over a window the instance names; $window is its own name
# for it, such as 30m.
usage-sub2api-window-limit = { $window } limit
usage-sub2api-today-requests = Today requests
# Tokens and their cost, such as 1,200 · $1.25.
usage-sub2api-today-tokens = Today tokens
usage-sub2api-all-time-requests = All time requests
usage-sub2api-all-time-tokens = All time tokens
# When the key or subscription expires; the value is a date.
usage-sub2api-expires = Expires
usage-sub2api-balance = Balance
usage-sub2api-usage-summary = Usage summary
