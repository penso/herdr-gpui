# The account menu's GitLab page: each GitLab instance the Orchestrator
# reads, the token it is read with, and connecting or pasting one. $host is an
# instance's address, such as gitlab.com; $user a GitLab username.

gitlab-menu-close = Close
gitlab-menu-no-store = This window does not keep GitLab tokens.

# Where an instance's token comes from, shown as $from in the lines below.
# $checkout is a folder's name.
gitlab-menu-from-envrc-of = from the .envrc of { $checkout }
gitlab-menu-from-envrc = from the project's .envrc
gitlab-menu-from-environment = from GITLAB_TOKEN
gitlab-menu-from-connected = connected here
gitlab-menu-from-saved = saved here
gitlab-menu-from-cli = from the GitLab CLI (glab)

gitlab-menu-plain = Read over plain HTTP, so no token is sent. Use an https:// remote for private projects.
gitlab-menu-looking = Looking for a token…
gitlab-menu-not-connected-note = Not connected. Public projects still list; connect for private ones.
gitlab-menu-no-token-note = No token. Public projects still list; set GITLAB_TOKEN in the project's .envrc, sign in with glab, or paste a token for private ones.
gitlab-menu-checking = Checking the token { $from }…
gitlab-menu-signed-in = Signed in as { $user }, { $from }.
gitlab-menu-refused-connect = GitLab refused the token { $from }. Connect again, or paste a new one.
gitlab-menu-refused = GitLab refused the token { $from }. Paste a new one.
gitlab-menu-environment-refused = GitLab refused GITLAB_TOKEN. Change or unset it in your shell: a token saved here is only tried after it.
gitlab-menu-envrc-refused = GitLab refused the token { $from }. Change it there: it is tried first.
gitlab-menu-failed = { $error }. The token is { $from }.
gitlab-menu-record-failed = { $error }. Saving or forgetting a token replaces them.
gitlab-menu-paste-first = Paste a personal access token first.
gitlab-menu-saved = Saved. GitLab at { $host } signs this token in as { $user }.
gitlab-menu-removed = Removed the GitLab sign-in kept for { $host }.
gitlab-menu-connected = Connected. GitLab at { $host } signs in as { $user }.

# A card's buttons.
gitlab-menu-connect-again = Connect again
gitlab-menu-connect = Connect
gitlab-menu-disconnect = Disconnect
gitlab-menu-forget = Forget

# A card's chip: where its token comes from, in a word or two.
gitlab-menu-chip-connected = connected
gitlab-menu-chip-saved = saved token
gitlab-menu-chip-none = no token
gitlab-menu-not-connected = Not connected
# Beside a card whose token is in a checkout's .envrc; $checkout is a folder.
gitlab-menu-from-folder = from { $checkout }/

# The page when no instance has a token yet.
gitlab-menu-connect-host = Connect { $host }
gitlab-menu-welcome-title = Connect your GitLab account
gitlab-menu-welcome-body = The Orchestrator reads issues from { $host }. Public projects list without an account; connect for private ones.
# $checkout is the folder whose remote names the instance.
gitlab-menu-welcome-body-for = The Orchestrator reads issues from { $host } for { $checkout }. Public projects list without an account; connect for private ones.
# $hosts lists other instances, joined by commas.
gitlab-menu-also-read = Also read: { $hosts }
gitlab-menu-use-token-instead = Use a personal access token instead
gitlab-menu-paste-token = Paste a personal access token
gitlab-menu-already-set-up = Already set up? GITLAB_TOKEN in a project's .envrc or your shell, and glab, are picked up on their own.

# Connecting through GitLab's device approval.
gitlab-menu-asking = Asking GitLab for a code…
gitlab-menu-approve-code = Approve this code on GitLab:
gitlab-menu-open = Open GitLab
gitlab-menu-copied = Copied
gitlab-menu-copy-code = Copy code
gitlab-menu-cancel = Cancel
gitlab-menu-waiting = This page updates once GitLab has the approval.

# The pasted-token form.
gitlab-menu-form-note = For an instance without Connect. The token needs the read_api scope; GitLab is asked whom it signs in as before it is saved, only ever over HTTPS. GITLAB_TOKEN in the project's .envrc or your shell is used before it.
gitlab-menu-instance = Instance
gitlab-menu-token = Personal access token
gitlab-menu-paste-for-instance = Paste a token for an instance…
gitlab-menu-form-cancel = Cancel
gitlab-menu-check-and-save = Check and save
gitlab-menu-add-instance = + Add instance
