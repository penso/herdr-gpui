# Settings > Cloud devices > Coder. User, workspace, and deployment names
# come from Coder and stay as they are.

settings-cloud-coder-note = Create Coder workspaces from the device picker and use them as devices. Register an OAuth2 app in Coder with the redirect URI below; the deployment must enable the oauth2 experiment.
settings-cloud-coder-not-configured = Not configured. Enter the deployment and its OAuth app, then save.
settings-cloud-coder-waiting-for-browser = Waiting for the browser…
settings-cloud-coder-finish-in-browser = Finish signing in in your browser…
settings-cloud-coder-not-signed-in = Not signed in
settings-cloud-coder-signed-in-as = Signed in as { $user }
# $reason is why Coder could not say who is signed in.
settings-cloud-coder-unreadable = Signed in; the account could not be read: { $reason }
settings-cloud-coder-sign-in = Sign in
settings-cloud-coder-sign-out = Sign out

## The deployment's fields, each a label and its placeholder.

settings-cloud-coder-url = Deployment URL
settings-cloud-coder-client-id = OAuth client ID
settings-cloud-coder-client-id-placeholder = The OAuth2 app's client ID
settings-cloud-coder-redirect = Redirect URI
settings-cloud-coder-organization = Organization
settings-cloud-coder-organization-placeholder = Optional; your default organization if empty
settings-cloud-coder-prefix = Workspace name prefix
settings-cloud-coder-prefix-placeholder = Optional; herdr if empty
settings-cloud-coder-cli = coder CLI
settings-cloud-coder-cli-placeholder = Optional; found on PATH if empty
settings-cloud-coder-secret = OAuth client secret
settings-cloud-coder-secret-environment = Set by HERDR_CODER_OAUTH_CLIENT_SECRET, which takes precedence.
settings-cloud-coder-secret-config = Set by [coder] oauth_client_secret in the config file, which takes precedence.
settings-cloud-coder-secret-saved = Saved. Type a new secret to replace it.
settings-cloud-coder-secret-missing = Not saved yet. Coder requires it to sign in.
settings-cloud-coder-forget-secret = Forget secret

## Coder devices.

settings-cloud-coder-devices = Coder devices
settings-cloud-coder-none-yet = None yet. Add one from Coder in Add Device….
settings-cloud-coder-remove-note = Remove takes a device off this list and leaves its workspace running in Coder. Destroy deletes the workspace at Coder.
settings-cloud-coder-destroy = Destroy…
settings-cloud-coder-destroy-confirm = Destroy workspace
# $workspace is the workspace's name at Coder.
settings-cloud-coder-destroy-warning = Destroy deletes the workspace { $workspace } and its disk at Coder, and removes it here. This cannot be undone.
settings-cloud-coder-destroy-contacting = Contacting Coder…
settings-cloud-coder-destroy-waiting = Waiting for the workspace's current build to finish…
settings-cloud-coder-destroy-queued = Queued at Coder…
settings-cloud-coder-destroy-deleting = Deleting the workspace…
# $name is the device's name.
settings-cloud-coder-destroyed = Destroyed { $name }.
settings-cloud-coder-destroy-failed = Could not destroy { $name }: { $error }
