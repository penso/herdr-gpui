# Wayfinder's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-wayfinder-setting-base-url = The Wayfinder gateway, started with `wayfinder-router serve`. Defaults to http://127.0.0.1:8088; another URL must be HTTPS, or plain HTTP on a loopback address.

## The report: the gateway's health, how it routed requests, and what that
## saved over the last 30 days. Route names and the gateway's own health
## word ($status, such as ok or degraded) stay as it sends them.

# The account line: how many models the local gateway serves.
usage-wayfinder-account = { $models ->
        [one] { $models } model · local gateway
       *[other] { $models } models · local gateway
    }
# The plan line while the gateway serves nothing live.
usage-wayfinder-offline = Offline mode
usage-wayfinder-dry-run = Dry run
# The plan line while some provider keys are missing; $missing counts them.
usage-wayfinder-degraded = { $missing ->
        [0] Degraded
        [one] Degraded, { $missing } key missing
       *[other] Degraded, { $missing } keys missing
    }
usage-wayfinder-local-gateway = Local gateway
usage-wayfinder-gateway = Gateway
# $status is the gateway's own word for its health; $mode says whether it
# runs offline, as a dry run, both, or live.
usage-wayfinder-gateway-state = { $status } · { $models ->
        [one] { $models } model
       *[other] { $models } models
    }{ $mode ->
        [offline] { " · " }offline
        [dry-run] { " · " }dry run
        [both] { " · " }offline · dry run
       *[live] {""}
    }
usage-wayfinder-routed = Routed
usage-wayfinder-saved = Saved, last 30 days
# $amount is the money saved, such as $4.12; $percent is how much cheaper
# than the costliest route, such as 73.3.
usage-wayfinder-saved-value = { $amount } · { $percent }% vs highest-cost route
# The same, for a gateway without prices, which saves in relative terms.
usage-wayfinder-saved-relative = { $percent }% vs highest-cost route
usage-wayfinder-decision = Avg decision
# $time is a number of milliseconds, such as 0.2.
usage-wayfinder-milliseconds = { $time } ms
usage-wayfinder-requests-by-route = Requests by route, last 30 days
