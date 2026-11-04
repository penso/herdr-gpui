# TypeSafe's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-typesafe-setting-cookie = Sign in at https://console.typesafe.ai/settings/billing, open Developer Tools > Application > Cookies > https://console.typesafe.ai, and copy every cookie there as one "name=value; name2=value2" header. An inference API key does not work here.

## The report: the balance, the billing cycle's spend, and the active
## credit grants. Plan names and the cycle's name stay as Typesafe sends
## them.

usage-typesafe-balance = Balance
usage-typesafe-spent = Spent
# $cycle is Typesafe's name for the billing cycle, such as September 2026.
usage-typesafe-spent-cycle = Spent ({ $cycle })
usage-typesafe-credits = Credits
usage-typesafe-credit = Credit
# A credit grant: $remaining dollars left of $amount, such as 4.98 of 5,
# and $expires the day it expires, such as Oct 19.
usage-typesafe-credit-left = { $remaining } of { $amount }, expires { $expires }
# How many more active credit grants there are; the value is a number.
usage-typesafe-additional-credits = Additional credits
