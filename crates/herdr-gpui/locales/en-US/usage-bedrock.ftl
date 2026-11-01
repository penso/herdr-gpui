# AWS Bedrock's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-bedrock-setting-profile = The AWS CLI profile to read Cost Explorer with, from ~/.aws/config on the selected host (run `aws sso login --profile <name>` first for SSO). Use "default" for the default credentials. The identity needs ce:GetCostAndUsage. AWS bills every Cost Explorer request, so this provider runs only once profile or budget is set here.
usage-bedrock-setting-budget = Optional monthly Bedrock budget in US dollars, e.g. 250, shown as a monthly limit. It does not cap AWS charges.
usage-bedrock-setting-aws-cli = Optional path to the AWS CLI v2 on the selected host when `aws` is not on its PATH, e.g. /opt/homebrew/bin/aws.
