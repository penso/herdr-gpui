# OpenAI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-openai-setting-api-key = An organization Admin API key (sk-admin-...), created by an organization owner at https://platform.openai.com/settings/organization/admin-keys. Project and service-account keys cannot read organization usage.
usage-openai-setting-project-id = Optional. A project ID (proj_...) from https://platform.openai.com/settings/organization/projects to limit spend and usage to that project.
usage-openai-setting-history-days = Optional. How many days of spend to total, 1 to 365. Defaults to 30.
usage-openai-setting-balance-fallback = Optional. Set to 1 to show the legacy prepaid credit balance when the key cannot read organization usage (older user API keys). Ignored with project_id.
