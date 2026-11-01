# Droid's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-factory-setting-api-key = A Factory API key (fk-…) from https://app.factory.ai/settings/api-keys.
usage-factory-setting-cookie = Used when no API key is set. Sign in to https://app.factory.ai, open Developer Tools > Application > Cookies > https://app.factory.ai, and copy the session cookies (wos-session, access-token, and any *.session-token) as "name=value; name2=value2".

## The report: the 5-hour, weekly, and monthly windows, or the older
## Standard and Premium token allowances, the extra usage balance, and the
## organization. Tier, plan, and organization names stay as Factory sends
## them.

# Windows of the Core token pool, beside the standard ones.
usage-factory-core-session = Core 5-hour
usage-factory-core-weekly = Core weekly
usage-factory-core-monthly = Core monthly
usage-factory-extra-usage = Extra usage balance
# The older billing's token allowances.
usage-factory-standard = Standard
usage-factory-premium = Premium
# The plan line with what happens past the limits: $plan is the tier and
# plan, such as Factory Team - Pro; $preference is Factory's word for the
# fallback, such as credits.
usage-factory-plan-fallback = { $plan } - Fallback: { $preference }
# The same, without a tier or plan.
usage-factory-fallback = Fallback: { $preference }
usage-factory-organization = Organization
# The organization's name.
usage-factory-name = Name
