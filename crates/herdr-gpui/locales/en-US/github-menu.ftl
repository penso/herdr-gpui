# The GitHub sign-in page of the account menu: the device's own account,
# saved SSH devices' accounts, and additional accounts. A letter in
# parentheses is the key that presses the button.

# $device is a saved device's name.
github-menu-host-own = Pull requests on { $device } use this account.
# $login is a GitHub login.
github-menu-host-main = Pull requests on { $device } use your main account @{ $login }. Sign in to use a different account for this device.
github-menu-host-signed-out = Sign in to view pull requests on { $device }.
github-menu-connected-cli = Connected with GitHub CLI
github-menu-connected = Connected
github-menu-copy-code-step = 1. Copy your one-time code
github-menu-copied = Copied
github-menu-copy-code = Copy code
github-menu-paste-code-step = 2. Open GitHub and paste the code
github-menu-checking = Checking GitHub account...
github-menu-ready = Your account is ready for pull request lookups.
github-menu-cli-available = GitHub CLI is signed in on this computer. Use its session, or sign in securely in your browser.
github-menu-sign-in-note = Sign in securely in your browser to view pull requests. No GitHub CLI required.
# $label is the first approval link's label, such as "Approve for @octocat".
github-menu-approve-key = { $label } (R)
github-menu-open = Open GitHub (O)
github-menu-use-cli = Use GitHub CLI authentication (G)
github-menu-try-again = Try again (S)
github-menu-use-another = Use another account (S)
github-menu-sign-in-browser = Sign in with browser (S)
github-menu-sign-in = Sign in (S)
github-menu-use-token = Use a token (T)
github-menu-add-account = Add account (A)
github-menu-save-token = Save token (Enter)
github-menu-cancel-token = Cancel (Esc)
github-menu-remove-account = Remove account (D)
github-menu-stop-cli = Stop using GitHub CLI (D)
github-menu-sign-out = Sign out (D)
github-menu-cli-no-repo-scope = GitHub CLI's token lacks the repo scope, so private repositories are unavailable. Run `gh auth refresh --scopes repo`.
github-menu-cli-session = Using GitHub CLI's sign-in for this app session. This platform has no credential store to remember the choice.
github-menu-cli-keeps-token = GitHub CLI keeps this token; Herdr UI saves none. After `gh auth logout`, this app signs out at its next check.

# Additional accounts.
github-menu-too-many-accounts = At most { $count } additional GitHub accounts can be added.
github-menu-main-account = Main account
github-menu-new-account = New account
github-menu-add-account-chip = + Add account (A)
github-menu-account-note = Sign in to an account that can see repositories your main account cannot, such as an Enterprise Managed User. It reads repositories owned by its login and organizations; the rest stay with your main account. An Enterprise Managed User cannot install Herdr's GitHub App, so use a personal access token for one.
# $owners lists GitHub owners, joined by commas.
github-menu-account-owners = Reads repositories owned by { $owners }. Everything else uses your main account. Add owners in [github] accounts.
github-menu-token-label = Personal access token
github-menu-token-help = Create one at github.com/settings/tokens as this account: a classic token with repo and read:org, or a fine-grained token with Contents, Issues, and Pull requests read access. It is checked with GitHub, then saved like a browser sign-in.

# What the GitHub App still needs from the user.
github-menu-install-title = Install the GitHub App
github-menu-install-body = Herdr's GitHub App is not installed for @{ $login } or any organization, so only public repositories load. Install it and choose the repositories it may read.
github-menu-install-custom-body = The GitHub App set as oauth_client_id is not installed for @{ $login } or any organization, so only public repositories load. Install it from that App's GitHub page.
github-menu-install-link = Install on GitHub
github-menu-approve-title = Approve GitHub App permissions
# $count is how many owners must approve.
github-menu-approve-body = { $count ->
        [one] Herdr's GitHub App asks for new permissions. Until it is approved, private repositories there cannot show checks, open pull requests, comment, or merge.
       *[other] Herdr's GitHub App asks for new permissions. Until they are approved, private repositories there cannot show checks, open pull requests, comment, or merge.
    }
# $owner is the user's own GitHub login.
github-menu-approve-own = Approve for @{ $owner }
# $owner is an organization.
github-menu-approve-organization = Approve for { $owner } (needs an owner of it)
# A toast: $body is one of the bodies above.
github-menu-attention-toast = { $body } Open the GitHub page from your avatar in the title bar.
github-menu-more-owners = And { $count } more on GitHub.
github-menu-recheck-note = Herdr checks again shortly after you open this page, and the dot on your avatar clears.
# The token field's placeholder: the prefixes GitHub tokens start with.
github-menu-token-placeholder = github_pat_… or ghp_…
