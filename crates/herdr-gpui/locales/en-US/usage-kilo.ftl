# Kilo's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-kilo-setting-api-key = A Kilo API key from https://app.kilo.ai (Profile → API keys). Without one, the Kilo CLI's sign-in in ~/.local/share/kilo/auth.json is used; run `kilo auth login` to create it.

## The report: credits left, Kilo Pass usage this billing period, and auto
## top-up. Plan names, such as Starter or Kilo Pass, and payment methods
## stay as Kilo sends them.

usage-kilo-credits-left = Credits left
usage-kilo-plan = Plan
# Kilo's monthly subscription, as a fact's label.
usage-kilo-pass = Kilo Pass
# $used and $base are dollar amounts, such as $0.00 and $19.00: the pass's
# usage of its base credits this period.
usage-kilo-pass-used = { $used } of { $base }
# The same, with bonus credits on top ($bonus, such as $9.50).
usage-kilo-pass-bonus = { $used } of { $base } (+ { $bonus } bonus)
usage-kilo-top-up = Auto top-up
# Whether auto top-up is on, when Kilo names no payment method.
usage-kilo-off = Off
usage-kilo-on = On
