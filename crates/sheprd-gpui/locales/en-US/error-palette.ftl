# Why a palette action or project lookup failed.

error-palette-command-changed = This command changed or was removed. Reopen the palette.
error-palette-connection-not-ready = The selected connection is not ready.
error-palette-destination-removed = This agent or terminal no longer exists. Reopen the palette.
error-palette-host-unavailable = This host is no longer connected. Reopen the palette.
error-palette-invalid-frecency = Invalid saved palette history
error-palette-local-changed = The local connection changed. Reopen the palette.
error-palette-no-session = No captured daemon session. Reopen the palette.
error-palette-pane-removed = The original pane no longer exists in its tab. Reopen the palette.
# $error is what git printed.
error-palette-project-clone = git clone failed: { $error }
error-palette-project-create = Could not create the project folder: { $error }
error-palette-project-exists = A folder of that name already exists in the project root
# palette.project_roots is a config key.
error-palette-project-limit = Project discovery reached its directory limit; narrow palette.project_roots
error-palette-project-path = Project paths must be absolute UTF-8 paths without control characters
error-palette-project-removed = Project directory is no longer available
error-palette-project-response = Unexpected workspace creation response. Review workspace state before retrying.
error-palette-project-root-missing = The first configured project root is not available
error-palette-project-roots = palette.project_roots must contain at most 16 nonempty paths of at most 8192 bytes
error-palette-project-state-changed = Workspace directories changed. Select the project again.
# $variable is the environment variable's name.
error-palette-project-variable = Project path references an unset environment variable: { $variable }
error-palette-session-changed = The daemon session changed. Reopen the palette.
error-palette-tab-removed = The original tab no longer exists in its workspace. Reopen the palette.
error-palette-workspace-removed = This workspace no longer exists. Reopen the palette.
