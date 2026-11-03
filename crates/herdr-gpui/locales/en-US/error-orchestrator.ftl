# The orchestrator: its issue and pull request records, their database,
# and the agents it starts. Database, Git, GitLab, and system text stays as
# it is in $error.

# $id is the text that was read, in quotes. bd is the Beads program.
error-orchestrator-bead-id = Bead id { $id } is not one bd accepts
error-orchestrator-beads-missing = Beads is not installed on this host (bd was not found)
error-orchestrator-busy = Several actions are already running; try again when one finishes
error-orchestrator-cancelled = Cancelled
# $error is the database's own description of the failure.
error-orchestrator-database = Orchestrator database failed: { $error }
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here.
error-orchestrator-failed = { $kind ->
    [create-worktree] Creating the worktree failed: { $error }
    [discover-agents] Discovering installed agents failed: { $error }
    [name-pane] Naming the agent pane failed: { $error }
    [name-tab] Naming the agent tab failed: { $error }
    [open-repository] Opening the repository failed: { $error }
    [read-base-commit] Reading the base commit failed: { $error }
    [read-bd-options] Reading bd's options failed: { $error }
    [read-beads] Reading Beads failed: { $error }
    [read-beads-blockers] Reading Beads blockers failed: { $error }
    [read-branches] Reading the branches failed: { $error }
    [read-checks] Reading pull request checks failed: { $error }
    [read-github] Reading GitHub failed: { $error }
    [read-repository] Reading the repository failed: { $error }
    [read-workspaces] Reading the workspaces failed: { $error }
    [remove-worktree] Removing the worktree failed: { $error }
    [resolve-beads-folder] Resolving the Beads folder failed: { $error }
    [send-message] Sending the message failed: { $error }
    [send-prompt] Sending the prompt failed: { $error }
    [start-agent] Starting the agent failed: { $error }
    [stop-agent] Stopping the agent failed: { $error }
   *[other] { $operation } failed: { $error }
}
error-orchestrator-host-cannot-take = That host cannot take this repository
error-orchestrator-host-not-shown = Show this repository's host to dispatch elsewhere
error-orchestrator-invalid-checkout = A saved Orchestrator tab names a folder that is not an absolute path
# $key is the text that was read, in quotes.
error-orchestrator-item-key = Not an item key: { $key }
# $column is a database column name.
error-orchestrator-json = Stored { $column } is not valid JSON: { $error }
error-orchestrator-menu-open = Close the open menu first
# $found and $known are database schema version numbers.
error-orchestrator-newer-schema = Shared data is schema { $found }; this herdr-gpui understands up to { $known }. Read-only until updated.
error-orchestrator-no-data-directory = No state directory is available for orchestrator state
error-orchestrator-not-a-repository = This folder is not inside a Git repository
# $run is a run's ID.
error-orchestrator-not-owner = Run { $run } belongs to another application
error-orchestrator-not-web-address = Only web addresses open here
error-orchestrator-nothing-to-open = This run has no Herdr workspace to show
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here.
error-orchestrator-output = { $kind ->
    [create-worktree] Creating the worktree returned unexpected output
    [discover-agents] Discovering installed agents returned unexpected output
    [name-pane] Naming the agent pane returned unexpected output
    [name-tab] Naming the agent tab returned unexpected output
    [open-repository] Opening the repository returned unexpected output
    [read-base-commit] Reading the base commit returned unexpected output
    [read-bd-options] Reading bd's options returned unexpected output
    [read-beads] Reading Beads returned unexpected output
    [read-beads-blockers] Reading Beads blockers returned unexpected output
    [read-branches] Reading the branches returned unexpected output
    [read-checks] Reading pull request checks returned unexpected output
    [read-github] Reading GitHub returned unexpected output
    [read-repository] Reading the repository returned unexpected output
    [read-workspaces] Reading the workspaces returned unexpected output
    [remove-worktree] Removing the worktree returned unexpected output
    [resolve-beads-folder] Resolving the Beads folder returned unexpected output
    [send-message] Sending the message returned unexpected output
    [send-prompt] Sending the prompt returned unexpected output
    [start-agent] Starting the agent returned unexpected output
    [stop-agent] Stopping the agent returned unexpected output
   *[other] { $operation } returned unexpected output
}
# $kind is the step that failed. $operation is its English description,
# shown for a step not listed here. $error is the JSON parser's own words.
error-orchestrator-output-json = { $kind ->
    [create-worktree] Creating the worktree returned invalid JSON: { $error }
    [discover-agents] Discovering installed agents returned invalid JSON: { $error }
    [name-pane] Naming the agent pane returned invalid JSON: { $error }
    [name-tab] Naming the agent tab returned invalid JSON: { $error }
    [open-repository] Opening the repository returned invalid JSON: { $error }
    [read-base-commit] Reading the base commit returned invalid JSON: { $error }
    [read-bd-options] Reading bd's options returned invalid JSON: { $error }
    [read-beads] Reading Beads returned invalid JSON: { $error }
    [read-beads-blockers] Reading Beads blockers returned invalid JSON: { $error }
    [read-branches] Reading the branches returned invalid JSON: { $error }
    [read-checks] Reading pull request checks returned invalid JSON: { $error }
    [read-github] Reading GitHub returned invalid JSON: { $error }
    [read-repository] Reading the repository returned invalid JSON: { $error }
    [read-workspaces] Reading the workspaces returned invalid JSON: { $error }
    [remove-worktree] Removing the worktree returned invalid JSON: { $error }
    [resolve-beads-folder] Resolving the Beads folder returned invalid JSON: { $error }
    [send-message] Sending the message returned invalid JSON: { $error }
    [send-prompt] Sending the prompt returned invalid JSON: { $error }
    [start-agent] Starting the agent returned invalid JSON: { $error }
    [stop-agent] Stopping the agent returned invalid JSON: { $error }
   *[other] { $operation } returned invalid JSON: { $error }
}
# $owner is the text that was read, in quotes.
error-orchestrator-owner = Unknown run owner { $owner }
# $provider is the text that was read, in quotes.
error-orchestrator-provider = Unknown issue provider { $provider }
error-orchestrator-pull-request-changed = The pull request changed since you confirmed; look at it again before merging
# $url is a Git remote's address, in quotes.
error-orchestrator-remote-url = Remote URL { $url } names no repository
error-orchestrator-run-not-found = Run { $run } was not found
error-orchestrator-teleport = Setting up the other host failed: { $error }
# $column is a database column name; $value is the text stored, in quotes.
error-orchestrator-time = Stored { $column } is not a valid time: { $value }
error-orchestrator-unsupported-host = This host cannot run the orchestrator's scripts
error-orchestrator-worker = Could not start the orchestrator's worker: { $error }
error-orchestrator-workspace-closed = This run's workspace is closed

## Files the orchestrator keeps private to this user.

error-orchestrator-private-not-private = { $path } is not a private file or directory owned by this user
error-orchestrator-private-prepare = Could not prepare { $path }: { $error }

## Reading issues and merge requests from GitLab. GITLAB_TOKEN is an
## environment variable, .envrc a file, read_api a token scope, and
## [gitlab] allow_plaintext_credentials a config key.

# $host is the text that was read, in quotes.
error-orchestrator-gitlab-address = GitLab's address for { $host } is not valid
error-orchestrator-gitlab-authentication = GitLab refused the token; replace it from the account button at the top right
error-orchestrator-gitlab-code-expired = The GitLab sign-in code expired; connect again
error-orchestrator-gitlab-denied = GitLab's sign-in was declined
error-orchestrator-gitlab-directory = The folder for saved GitLab tokens could not be found: { $error }
error-orchestrator-gitlab-environment-refused = GitLab refused GITLAB_TOKEN; change or unset it, since it is tried before a token saved in the app
error-orchestrator-gitlab-envrc-refused = GitLab refused the GITLAB_TOKEN this checkout's .envrc sets; change it there, since it is tried first
# $repository is a GitLab project path, such as group/project.
error-orchestrator-gitlab-forbidden = GitLab refused access to { $repository }; the token needs the read_api scope
# $host is the text that was read, in quotes.
error-orchestrator-gitlab-host = { $host } is not a GitLab host name
error-orchestrator-gitlab-insecure-host = GitLab at { $host } is read over plain HTTP, and a token is only ever sent over HTTPS: use an https:// remote
error-orchestrator-gitlab-insecure-remote = GitLab shows { $repository } only when signed in, and a token is never sent over plain HTTP: use an https:// remote
error-orchestrator-gitlab-json = GitLab's answer is not the expected JSON: { $error }
# $origin is the scheme and host tried, such as https://gitlab.com.
error-orchestrator-gitlab-network = GitLab could not be reached at { $origin }: { $error }
error-orchestrator-gitlab-no-keyring = No desktop keyring is running to save a GitLab token in. Start GNOME Keyring, KWallet, or KeePassXC, set [gitlab] allow_plaintext_credentials = true, or use GITLAB_TOKEN
error-orchestrator-gitlab-no-store = No secure credential store is available to save a GitLab token in. Set [gitlab] allow_plaintext_credentials = true, or use GITLAB_TOKEN
error-orchestrator-gitlab-not-found = GitLab shows no project { $repository } to this token
error-orchestrator-gitlab-rate-limit = GitLab rate limit reached; it is read again later
error-orchestrator-gitlab-read = GitLab's answer could not be read: { $error }
error-orchestrator-gitlab-record = The saved GitLab tokens are not in the expected form
error-orchestrator-gitlab-scope = GitLab refused the token's access; it needs the read_api scope
error-orchestrator-gitlab-sign-in = GitLab shows { $repository } only when signed in: add a token from the account button at the top right
error-orchestrator-gitlab-size = GitLab's answer exceeded the size limit
# $status is the HTTP status code.
error-orchestrator-gitlab-status = GitLab request failed (HTTP { $status })
error-orchestrator-gitlab-store = The GitLab token could not be read or saved: { $error }
error-orchestrator-gitlab-token = The GitLab token is not one an HTTP header can carry
error-orchestrator-gitlab-too-many-hosts = GitLab tokens are saved for at most { $limit } hosts
