# Manus's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-manus-setting-session-token = Sign in to https://manus.im, open Developer Tools > Application > Cookies > https://manus.im, and copy the value of the session_id cookie (the value only, without "session_id="). Not needed when Manus is listed in show_providers and you are signed in with Chrome or Safari.
usage-manus-setting-cookie = Instead of session_token, a whole manus.im Cookie header that includes session_id, as "session_id=value; …".

## The report: the credit balance, the plan's monthly credits, and the
## credits that refresh on the service's interval. The interval's name
## (such as Daily) stays as the service sends it.

usage-manus-monthly-credits = Monthly credits
usage-manus-daily-refresh = Daily refresh
usage-manus-free = Free
usage-manus-monthly = Monthly
# The refreshing credits, when the service names no interval.
usage-manus-refresh = Refresh
# $left credits are left of $total, both counts such as 3,000 and 4,000.
usage-manus-left-of = { $left } of { $total }
usage-manus-balance = Balance
usage-manus-credits = Credits
