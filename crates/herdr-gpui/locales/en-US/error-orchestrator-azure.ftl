# Why an Azure DevOps repository's pull requests or its project's work items could not be read.
# AZURE_DEVOPS_EXT_PAT is an environment variable, and `az login` a command: keep both as they are.

error-orchestrator-azure-address = Azure DevOps project address for { $repository } is not valid
error-orchestrator-azure-authentication = Azure DevOps rejected the token; sign in again with `az login`, or replace AZURE_DEVOPS_EXT_PAT
error-orchestrator-azure-forbidden = The token cannot read { $repository }; it needs the Code (Read) and Work Items (Read) scopes
error-orchestrator-azure-json = Azure DevOps's answer is not the expected JSON: { $error }
# $origin is the address tried, https://dev.azure.com.
error-orchestrator-azure-network = Azure DevOps could not be reached at { $origin }: { $error }
# Acting on a pull request listed from Azure DevOps alone, with no checkout of its repository open.
error-orchestrator-azure-no-checkout = Open a checkout of { $repository } as a workspace to act on its pull requests
error-orchestrator-azure-not-found = Azure DevOps has no repository { $repository }, or the token cannot see it
error-orchestrator-azure-rate-limit = Azure DevOps rate limit reached; it is read again later
error-orchestrator-azure-read = Azure DevOps's answer could not be read: { $error }
error-orchestrator-azure-reader = Azure DevOps did not say who the token belongs to, so its pull requests cannot be found
error-orchestrator-azure-sign-in = Sign in to read { $repository }: run `az login`, or set AZURE_DEVOPS_EXT_PAT in your shell
error-orchestrator-azure-size = Azure DevOps's answer exceeded the size limit
error-orchestrator-azure-status = Azure DevOps request failed (HTTP { $status })
error-orchestrator-azure-token = The Azure DevOps token is not one an HTTP header can carry
