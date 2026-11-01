# The account menu's Azure DevOps page: each organization the Orchestrator
# reads, the token it is read with, and pasting one. $organization is an
# Azure DevOps organization's name, such as contoso; $user the name Azure
# DevOps shows for an account. AZURE_DEVOPS_EXT_PAT is an environment
# variable and `az` a command: keep both as they are.

azure-devops-menu-help = Paste a personal access token to read pull requests and work items. It needs the Code (Read) and Work Items (Read) scopes.
azure-devops-menu-no-store = This window does not keep Azure DevOps tokens.
azure-devops-menu-organization = Organization
azure-devops-menu-token = Personal access token
azure-devops-menu-create-token = Create a token in Azure DevOps
azure-devops-menu-check-and-save = Check and save
azure-devops-menu-forget = Forget
azure-devops-menu-organization-first = Type the organization's name first.
azure-devops-menu-paste-first = Paste a token first.

# Where an organization's token comes from, shown as $from in the lines below.
azure-devops-menu-from-environment = from AZURE_DEVOPS_EXT_PAT
azure-devops-menu-from-saved = saved here
azure-devops-menu-from-cli = from the Azure CLI (az)

# Shown as $projects in the lines below.
azure-devops-menu-projects =
    { $count ->
        [one] 1 project
       *[other] { $count } projects
    }
azure-devops-menu-projects-hidden = the token does not list projects

azure-devops-menu-looking = Looking for a token…
azure-devops-menu-no-token = No token. Paste one below.
azure-devops-menu-checking = Checking the token { $from }…
azure-devops-menu-signed-in = Signed in, { $from }; { $projects }.
azure-devops-menu-refused = Azure DevOps refused the token { $from }. Paste a new one.
azure-devops-menu-environment-refused = Azure DevOps refused AZURE_DEVOPS_EXT_PAT. Change or unset it: it is tried before a token saved here.
azure-devops-menu-failed = { $error } The token is { $from }.
azure-devops-menu-record-failed = { $error } Saving a token replaces them.
azure-devops-menu-saved = Saved for { $organization }, signed in as { $user }; { $projects }.
azure-devops-menu-removed = Forgot the token for { $organization }.

# The Pull requests section's banner while an Azure DevOps repository waits
# for a token; clicking it opens this page.
azure-devops-menu-connect-banner = Connect Azure DevOps to read { $repository }
