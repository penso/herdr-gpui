# Groq's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-groq-setting-api-key = A GroqCloud Enterprise API key from https://console.groq.com/keys. Only Enterprise organizations can read Prometheus metrics.
usage-groq-setting-base-url = Optional HTTPS API base URL for a private gateway. Defaults to https://api.groq.com/v1.

## The report: request and token rates over the last five minutes, from
## GroqCloud's Prometheus metrics, an Enterprise feature.

# The plan line: where the rates come from.
usage-groq-prometheus = Prometheus metrics
usage-groq-last-5-minutes = Last 5 minutes
usage-groq-requests = Requests / min
usage-groq-tokens = Tokens / min
usage-groq-cache-hits = Cache hits / min
usage-groq-metrics = Metrics
usage-groq-unavailable = Unavailable
# Why a key that is not an Enterprise key shows no rates.
usage-groq-enterprise-only = Prometheus metrics need a GroqCloud Enterprise key
