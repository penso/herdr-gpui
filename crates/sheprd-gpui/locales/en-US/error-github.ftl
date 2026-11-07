# GitHub failures: sign-in, its requests, the GitHub CLI, pull request
# lookups, and pull request actions. GitHub's own words, and the system's,
# stay as they are in $error and $reason.

error-github-authentication = GitHub authentication required. Use menu > GitHub sign-in or set GH_TOKEN / GITHUB_TOKEN.
error-github-authorization = GitHub authorization failed. Check OAuth application settings.
# HERDR_GITHUB_OAUTH_CLIENT_ID is an environment variable.
error-github-client-id-encoding = HERDR_GITHUB_OAUTH_CLIENT_ID must be UTF-8
error-github-denied = GitHub authorization denied.
error-github-device = Invalid GitHub device authorization response.
error-github-encoding = Invalid GitHub credential encoding.
error-github-expired = GitHub code expired. Sign in again.
error-github-forbidden = GitHub denied access: check token permissions or SSO authorization.
error-github-header = Invalid GitHub authorization header.
# $key is the config key or environment variable that holds the client ID,
# such as github.oauth_client_id.
error-github-invalid-client-id = { $key } must be 1..256 ASCII letters, digits, '.', '_' or '-' (public client ID, not a secret)
error-github-json = Invalid GitHub JSON response.
# $error is the network library's own description of the failure.
error-github-network = GitHub network request failed or timed out ({ $error }).
error-github-query = GitHub query failed. Check token repository permissions and rate limits.
error-github-rate-limit = GitHub rate limit reached. Retry later.
# $time is the local time, hours and minutes, when GitHub's quota refills.
error-github-rate-limit-until = GitHub rate limit reached. Retry after { $time }.
error-github-read = GitHub response could not be read within the size/time limit.
# $reason is GitHub's own text.
error-github-rejected = GitHub refused the request: { $reason }
# $repository is owner/name. $sign_in is how the user signed in: app (a
# GitHub App) or token (any other token).
error-github-repository-hidden = { $sign_in ->
    [app] GitHub cannot see { $repository } with this sign-in. This GitHub App sign-in only reaches repositories the App is installed on: install it for the repository's owner and approve the permissions it asks for, or add the account that owns it on the GitHub page.
   *[token] GitHub cannot see { $repository } with this sign-in. Check the remote, the token's repository access, and SSO authorization, or add the account that owns it on the GitHub page.
}
error-github-size = GitHub response exceeded the size limit.
# $status is the HTTP status code.
error-github-status = GitHub request failed (HTTP { $status }). Check network and repository access.
error-github-token = Invalid GitHub token. Replace the configured credential.
error-github-token-type = Unsupported GitHub token type.
# $kind names the background task that stopped. $worker is its English name,
# shown for a task not listed here.
error-github-worker = { $kind ->
    [authentication] GitHub authentication worker stopped.
    [list] GitHub list worker stopped.
    [profile] GitHub profile worker stopped.
   *[other] GitHub { $worker } worker stopped.
}

## The [github] accounts config list. [github] and accounts are config names.

error-github-accounts-duplicate-id = [github] accounts lists the id { $id } twice
error-github-accounts-id = [github] accounts id must be 32 lowercase hexadecimal digits
error-github-accounts-not-a-list = [github] accounts must be a list of account tables
# $owner is the name as written in the config, in quotes.
error-github-accounts-owner = [github] accounts owner { $owner } is not a GitHub user or organization name
error-github-accounts-random = Could not generate a GitHub account ID
error-github-accounts-too-many = [github] accounts may list at most { $limit } accounts

## The GitHub CLI (gh). `gh auth login` and `gh auth status` are commands.

error-github-cli-missing = GitHub CLI (gh) was not found. Install it and run `gh auth login`, or sign in here.
error-github-cli-process = Could not run the GitHub CLI (gh).
error-github-cli-rejected = GitHub rejected the GitHub CLI's token. Run `gh auth login` to renew it, then try again.
error-github-cli-signed-out = GitHub CLI is not signed in to github.com. Run `gh auth login`, then try again.
error-github-cli-timeout = GitHub CLI did not answer within 10 seconds. Check `gh auth status`, then try again.

## Finding a branch's pull request.

error-github-pull-request-absolute-path = Daemon did not provide an absolute checkout and repository key.
error-github-pull-request-ambiguous = Multiple PRs match this branch; no PR selected.
error-github-pull-request-ambiguous-worktree = Multiple local worktrees match the daemon branch.
error-github-pull-request-branch = No supported branch available.
error-github-pull-request-branch-changed = Checkout branch changed. Waiting for daemon metadata.
error-github-pull-request-cancelled = PR lookup cancelled.
error-github-pull-request-checkout = Local checkout unavailable or not a trusted Git repository.
error-github-pull-request-encoding = Invalid process text.
error-github-pull-request-identity = PR identity does not match the repository and branch.
error-github-pull-request-metadata = No repository metadata.
error-github-pull-request-missing-worktree = No local worktree matches the daemon branch.
error-github-pull-request-origin = PR lookup supports GitHub.com origins only.
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here.
error-github-pull-request-process = { $kind ->
    [configure-errors] Could not configure process errors for Git PR lookup.
    [configure-output] Could not configure process output for Git PR lookup.
    [create-channel] Could not create process output channel for Git PR lookup.
    [launch-git] Could not launch Git (install git on PATH) for Git PR lookup.
    [read-output] Could not read process output for Git PR lookup.
    [wait] Could not wait for process for Git PR lookup.
   *[other] Could not { $operation } for Git PR lookup.
}
error-github-pull-request-repository = GitHub repository unavailable. Check repository access and token permissions.
error-github-pull-request-repository-mismatch = Local repository does not match daemon metadata.
error-github-pull-request-size = PR response exceeded the size limit.
error-github-pull-request-timeout = PR lookup timed out (15 seconds).
error-github-pull-request-untrusted-endpoint = PR lookup requires your owned local session socket or a saved SSH device. Other socket locations are unsupported.
error-github-pull-request-workspace-repository = Workspace directory is not in a Git repository.
error-github-pull-request-worktree-lookup = Local repository unavailable for worktree lookup.

## Commenting on, merging, or reading reviews of a pull request.

error-github-pr-actions-busy = A pull request action is already running.
error-github-pr-actions-comment-body = Enter a comment of at most 4096 characters.
error-github-pr-actions-merge-changed = The branch changed since this dialog opened. Review the pull request again.
error-github-pr-actions-merge-method = The repository does not allow this merge method.
error-github-pr-actions-process = Could not start the pull request worker.
error-github-pr-actions-target = This pull request cannot be acted on here: it is not open, or its details are incomplete. Refresh and try again.
error-github-pr-actions-worker = Pull request worker stopped. Check the pull request on GitHub before retrying.
