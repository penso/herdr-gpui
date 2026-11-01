# Why an Azure DevOps repository's pull requests or its project's work items could not be read.
# AZURE_DEVOPS_EXT_PAT is an environment variable, `az login` a command, and
# [azure_devops] allow_plaintext_credentials = true a setting: keep them as they are.

error-orchestrator-azure-address = Azure DevOps project address for { $repository } is not valid
error-orchestrator-azure-authentication = Azure DevOps rejected the token; replace it from the account menu, sign in again with `az login`, or replace AZURE_DEVOPS_EXT_PAT
error-orchestrator-azure-directory = The folder for saved Azure DevOps tokens could not be found: { $error }
error-orchestrator-azure-environment-refused = Azure DevOps refused AZURE_DEVOPS_EXT_PAT; change or unset it, since it is tried before a token saved in the app
error-orchestrator-azure-forbidden = The token cannot read { $repository }; it needs the Code (Read) and Work Items (Read) scopes
error-orchestrator-azure-json = Azure DevOps's answer is not the expected JSON: { $error }
# $origin is the address tried, https://dev.azure.com.
error-orchestrator-azure-network = Azure DevOps could not be reached at { $origin }: { $error }
error-orchestrator-azure-no-keyring = No desktop keyring is running to save an Azure DevOps token in. Start GNOME Keyring, KWallet, or KeePassXC, set [azure_devops] allow_plaintext_credentials = true, or use AZURE_DEVOPS_EXT_PAT
error-orchestrator-azure-no-organization = Azure DevOps has no organization { $organization }, or the token cannot see it
error-orchestrator-azure-no-store = No secure credential store is available to save an Azure DevOps token in. Set [azure_devops] allow_plaintext_credentials = true, or use AZURE_DEVOPS_EXT_PAT
error-orchestrator-azure-not-found = Azure DevOps has no repository { $repository }, or the token cannot see it
error-orchestrator-azure-organization = { $organization } is not an Azure DevOps organization name
error-orchestrator-azure-rate-limit = Azure DevOps rate limit reached; it is read again later
error-orchestrator-azure-read = Azure DevOps's answer could not be read: { $error }
error-orchestrator-azure-record = The saved Azure DevOps tokens are not in the expected form
error-orchestrator-azure-sign-in = Connect Azure DevOps from the account menu to read { $repository }, set AZURE_DEVOPS_EXT_PAT, or run `az login`
error-orchestrator-azure-size = Azure DevOps's answer exceeded the size limit
error-orchestrator-azure-status = Azure DevOps request failed (HTTP { $status })
error-orchestrator-azure-store = The Azure DevOps token could not be read or saved: { $error }
error-orchestrator-azure-token = The Azure DevOps token is not one an HTTP header can carry
error-orchestrator-azure-too-many-organizations = Azure DevOps tokens are saved for at most { $limit } organizations
