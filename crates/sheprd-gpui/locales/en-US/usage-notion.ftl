# Notion AI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-notion-setting-cookie = Your Notion session cookie. Sign in at https://app.notion.com, open Developer Tools > Application > Cookies for https://app.notion.com (or notion.so), copy the token_v2 cookie and paste it as "token_v2=value" (add "; notion_user_id=value" for accounts signed in to several users).
usage-notion-setting-workspace-id = The workspace (space) ID to read, dashed or not, for accounts in several workspaces. Defaults to the first Business or Enterprise workspace. It is the x-notion-space-id header of requests in Developer Tools > Network.

## The report: the workspace's rolling and billing-period AI allowances.
## Workspace and tier names, and Notion's enforcement word (such as
## Preview), stay as Notion sends them.

# A rolling window longer than six hours.
usage-notion-rolling = Rolling
usage-notion-workspace = Workspace
usage-notion-allowance = Allowance
usage-notion-not-tracked = Not tracked; allowances apply to Business and Enterprise workspaces
usage-notion-enforcement = Enforcement
