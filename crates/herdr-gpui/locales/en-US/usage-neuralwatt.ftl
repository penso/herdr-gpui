# Neuralwatt's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-neuralwatt-setting-api-key = A Neuralwatt API key, created or copied at https://portal.neuralwatt.com/dashboard.
usage-neuralwatt-setting-base-url = Optional HTTPS API URL for a proxy. Defaults to https://api.neuralwatt.com.

## The report: the subscription's energy allowance, the key's spending
## allowance, prepaid credits, and this month's spend. Plan names, the
## allowance period ($period, such as Monthly), and the accounting method
## stay as Neuralwatt sends them.

# A window for a subscription billed by the year.
usage-neuralwatt-yearly = Yearly
usage-neuralwatt-energy = Energy
# $used and $included are kilowatt-hours, such as 13.90 and 20.
usage-neuralwatt-energy-used = { $used } / { $included } kWh
# The key's spending allowance, by its period.
usage-neuralwatt-key-period = Key { $period }
usage-neuralwatt-key-allowance = Key Allowance
usage-neuralwatt-this-month = This month
# This month's spend: $cost is an amount, such as $4.20; $requests is the
# request count, such as 1,234, and $count the same as a number.
usage-neuralwatt-month-requests = { $count ->
        [one] { $cost } · { $requests } request
       *[other] { $cost } · { $requests } requests
    }
usage-neuralwatt-credits-used = Credits used
# $used and $total are amounts, such as $17.32 and $50.00.
usage-neuralwatt-credits-of = { $used } of { $total }
# The plan line; $plan is the plan's name, such as Standard.
usage-neuralwatt-plan = { $plan } plan
usage-neuralwatt-prepaid-credits = Prepaid credits
