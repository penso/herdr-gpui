# Muse Code's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-muse-setting-token = A Muse CLI device-code access token (it starts with dca:), used when this host has no `muse login`. It is providers.meta.access_token in ~/.config/muse/auth.json on a machine where you ran `muse login`. On a Mac where `muse login` kept it in the keychain instead, list muse in show_providers and press Allow in its usage panel rather than setting this. Dashboard LLM_ keys and LLM| inference keys are not accepted.

## The report: the subscription's rolling and weekly windows, or why the
## login has none. The plan's name stays as Muse sends it.

# The plan line when the login names no plan.
usage-muse-login = Muse login
usage-muse-subscription = Subscription
usage-muse-status = Status
usage-muse-payment-required = Payment method required; finish billing at https://dev.meta.ai
usage-muse-no-subscription = No active subscription on this login
usage-muse-no-quota = Quota not included in this login response
# A rolling window other than 5 hours; $hours is its length, such as 2.
usage-muse-hours = { $hours ->
        [one] { $hours } hour
       *[other] { $hours } hours
    }
