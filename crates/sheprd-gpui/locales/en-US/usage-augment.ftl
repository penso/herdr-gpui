# Augment's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-augment-setting-cookie = Your app.augmentcode.com browser session, used when the Auggie CLI is not signed in. Sign in at https://app.augmentcode.com, open Developer Tools → Application → Cookies → https://app.augmentcode.com, and copy the session cookies (such as `_session`, `session`, `web_rpc_proxy_session`, or `auth0`), pasted as one header: "name=value; name2=value2".

## The report: the billing cycle's credits. Plan names, such as Max Plan,
## stay as Augment or auggie writes them.

# The balance: credits left of the cycle's allowance.
usage-augment-credits-left = Credits left
# The plan line when auggie names no plan: the monthly allowance, where
# $amount is a grouped number such as 450,000.
usage-augment-credits-per-month = { $amount } credits/month
