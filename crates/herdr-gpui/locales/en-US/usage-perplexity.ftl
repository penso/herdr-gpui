# Perplexity's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-perplexity-setting-cookie = The perplexity.ai web session. Sign in at https://www.perplexity.ai, open Developer Tools → Application → Cookies → https://www.perplexity.ai, copy the session cookie (__Secure-next-auth.session-token, or __Secure-authjs.session-token on newer sign-ins), and paste it as "name=value".
usage-perplexity-setting-session-token = Alternatively, only the value of that session cookie; each session cookie name is tried in turn.

## The report: recurring, purchased, and bonus credits. Plan names (Pro,
## Max) stay as Perplexity names them.

usage-perplexity-credits = Credits
usage-perplexity-bonus-credits = Bonus credits
usage-perplexity-purchased-credits = Purchased credits
# Recurring credits used this month; $used and $total are grouped numbers,
# such as 2,750 and 10,000.
usage-perplexity-used-of = { $used } of { $total }
# When the earliest bonus credits expire; the value is a date.
usage-perplexity-bonus-expires = Bonus expires
