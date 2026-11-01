# Poe's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-poe-setting-api-key = A Poe API key, created or copied at https://poe.com/api/keys.

## The report: the point balance and the last 30 days of points history.
## Model (bot) names stay as Poe sends them.

usage-poe-points = Points
usage-poe-current-balance = Current balance
usage-poe-today = Today
usage-poe-last-7-days = Last 7 days
usage-poe-last-30-days = Last 30 days
usage-poe-top-model = Top model
usage-poe-points-by-model = Points by model, last 30 days
# A number of points; $points is the number, and $amount is it as shown,
# such as 1,500 or 42.5.
usage-poe-point-count = { $points ->
        [one] { $amount } point
       *[other] { $amount } points
    }
# A period's spend; $points is usage-poe-point-count, $requests the number
# of requests, and $count it grouped, such as 1,204.
usage-poe-summary = { $points } · { $requests ->
        [one] { $count } request
       *[other] { $count } requests
    }
# The same, with $cost the period's cost in dollars, such as $0.09.
usage-poe-summary-cost = { $points } · { $requests ->
        [one] { $count } request
       *[other] { $count } requests
    } · { $cost }
# The model that spent the most; $model is its name, $points
# usage-poe-point-count.
usage-poe-model-points = { $model } · { $points }
