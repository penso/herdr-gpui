# OpenAI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-openai-setting-api-key = An organization Admin API key (sk-admin-...), created by an organization owner at https://platform.openai.com/settings/organization/admin-keys. Project and service-account keys cannot read organization usage.
usage-openai-setting-project-id = Optional. A project ID (proj_...) from https://platform.openai.com/settings/organization/projects to limit spend and usage to that project.
usage-openai-setting-history-days = Optional. How many days of spend to total, 1 to 365. Defaults to 30.
usage-openai-setting-balance-fallback = Optional. Set to 1 to show the legacy prepaid credit balance when the key cannot read organization usage (older user API keys). Ignored with project_id.

## The report: the organization's API spend and completion usage from the
## Admin API, or the legacy credit-grant balance. Line items, models, and
## project IDs stay as OpenAI sends them.

# The spend balance over the history setting's $days days.
usage-openai-spend-days = { $days ->
        [1] Spend today
       *[other] Spend, last { $days } days
    }
usage-openai-spend = Spend
usage-openai-today = Today
usage-openai-last-7-days = Last 7 days
usage-openai-completions = Completions
usage-openai-requests = Requests
usage-openai-input-tokens = Input tokens
usage-openai-cached-input-tokens = Cached input tokens
usage-openai-output-tokens = Output tokens
usage-openai-spend-by-line-item = Spend by line item
# Spend the Admin API names no line item for.
usage-openai-unnamed-line-item = API
usage-openai-tokens-by-model = Tokens by model
# Tokens the Admin API names no model for.
usage-openai-unnamed-model = Responses and Chat Completions
# The plan line: read with an Admin API key, for every project or for
# $project, a project ID such as proj_abc.
usage-openai-admin-api = Admin API
usage-openai-admin-api-project = Admin API: { $project }
# The plan line of the legacy balance fallback.
usage-openai-api-balance = API balance
usage-openai-api-credits-left = API credits left
usage-openai-used = Used
usage-openai-next-expiry = Next expiry
usage-openai-credit-grants = Credit grants
