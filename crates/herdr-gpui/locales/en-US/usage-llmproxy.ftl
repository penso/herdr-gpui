# LLM Proxy's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-llmproxy-setting-api-key = The API key your LLM-API-Key-Proxy accepts (its PROXY_API_KEY), sent as a bearer token to base_url.
usage-llmproxy-setting-base-url = The proxy's URL, with or without /v1, e.g. https://proxy.example.com. It must be HTTPS unless it is on localhost, a private network, or a .local host.

## The report: the proxy's quota, request and token totals, and its busiest
## upstream providers. Provider names (such as openai) stay as the proxy
## names them.

# A window: the least quota left across the proxy's credentials.
usage-llmproxy-quota = Quota
# The plan line: $active of the proxy's $keys credentials are active, such
# as 3/4.
usage-llmproxy-active-keys = { $active }/{ $keys } active keys
usage-llmproxy-totals = Totals
usage-llmproxy-requests = Requests
usage-llmproxy-tokens = Tokens
# A section's title: the providers with the most requests.
usage-llmproxy-providers = Providers
# A provider's figures: $requests and $tokens are counts, such as 120 and
# 6,000; $cost is its approximate spend, such as $12.50.
usage-llmproxy-provider = { $requests } req · { $tokens } tok
usage-llmproxy-provider-cost = { $requests } req · { $tokens } tok · { $cost }
usage-llmproxy-approx-spend = Approx. spend
