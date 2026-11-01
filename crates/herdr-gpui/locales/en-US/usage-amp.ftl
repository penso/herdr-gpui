# Amp's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-amp-setting-api-key = An Amp access token, created in Amp's settings at https://ampcode.com/settings. Not needed when the Amp CLI is installed and signed in on the host (`amp login`).
usage-amp-setting-cookie = Your ampcode.com browser session, used when neither the CLI nor a token is available; it shows Amp Free only. Sign in at https://ampcode.com, open Developer Tools → Application → Cookies → https://ampcode.com, and copy the `session` cookie as "session=value".

## The report: the subscription's monthly allowances, credits, and
## workspace balances. Plan names, including Amp Free, and organization
## names stay as Amp sends them.

# A limit: how much of the month's Orb allowance is used.
usage-amp-orb-usage = Orb usage
usage-amp-monthly-allowances = Monthly allowances
# The agent allowance left this month; the value is a dollar amount.
usage-amp-agent = Agent
# Orb machine hours left this month on the a1.small size.
usage-amp-orb-hours = Orb (a1.small hours)
# $hours is a whole number of hours, such as 732.
usage-amp-hours = { $hours }h
usage-amp-under-an-hour = < 1h
usage-amp-individual-credits = Individual credits
# A workspace's credit balance; $name is the workspace's name.
usage-amp-workspace = Workspace { $name }
usage-amp-account = Account
usage-amp-organization = Organization
