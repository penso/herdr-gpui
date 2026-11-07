# Gemini's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-gemini-setting-client-id = The Gemini CLI's OAuth client id, used to refresh an expired sign-in. Normally found in the installed CLI (OAUTH_CLIENT_ID in @google/gemini-cli-core/dist/src/code_assist/oauth2.js); set it only when the CLI is not on the host's PATH.
usage-gemini-setting-client-secret = The matching OAUTH_CLIENT_SECRET from the same file.

## The report: each model family's daily quota and each model's share left.
## Model and family names, such as gemini-2.5-pro or Flash Lite, and a paid
## tier's name stay as Google sends them.

# The plan line for Code Assist's tiers.
usage-gemini-paid = Paid
usage-gemini-workspace = Workspace
usage-gemini-free = Free
usage-gemini-legacy = Legacy
usage-gemini-models = Models
# $percent is the share of a model's quota left, such as 40.
usage-gemini-left = { $percent }% left
