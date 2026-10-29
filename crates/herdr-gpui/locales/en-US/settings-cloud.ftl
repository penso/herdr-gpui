# Settings > Cloud devices: what the page and every provider's card say
# around their own fields. Provider, machine, region, size, and image names
# come from the providers and stay as they are. The section and the New
# device setup card take their titles from settings.ftl.

settings-cloud-unavailable = Cloud devices are unavailable on Windows. Accounts can still be edited here for other systems.
settings-cloud-remove = Remove
settings-cloud-none-yet = None yet.

## Shared by the provider cards.

settings-cloud-checking = Checking…
# A provider card's title, for a provider still being tried out.
settings-cloud-experimental = { $provider } (experimental)
settings-cloud-save = Save
settings-cloud-cancel = Cancel
settings-cloud-install-herdr = Install Herdr when missing
settings-cloud-install-herdr-in-it = Install Herdr in it when missing
settings-cloud-new-sandbox = New sandbox
settings-cloud-create-sandbox = Create sandbox
settings-cloud-finish-setup = Finish setup
# A job the main window runs; $name is the new device's name.
settings-cloud-creating = Creating { $name }. The device picker shows its progress.
settings-cloud-setting-up =Setting up { $name }. The device picker shows its progress.
settings-cloud-no-window-sandbox = Open a main Herdr window to create a sandbox.
# An account's API key or token, kept in the credential store.
settings-cloud-api-key = API key
settings-cloud-forget-key = Forget key
settings-cloud-key-saved = Saved. Type a new key to replace it.
settings-cloud-key-missing = No API key saved.
settings-cloud-key-unreadable = The API key could not be read: { $error }
settings-cloud-key-unusable = The API key could not be used: { $error }
settings-cloud-key-store-unavailable = Credential storage is unavailable. Saved devices can still be removed.
# The default among size or image choices; $choice is its name.
settings-cloud-default-choice = { $choice } (default)
# Deleting a device's machine at its provider; $name is the device's name.
settings-cloud-remove-ask = Remove…
settings-cloud-remove-from-herdr = Remove from Herdr only
settings-cloud-deleted = Deleted { $name }.
settings-cloud-delete-failed = Could not delete { $name }: { $error }

## New device setup.

settings-cloud-setup-run = Run the setup script when it connects
settings-cloud-setup-note = Runs once, in a setup tab, the first time a new cloud device connects: the agents switched on here, then the setup script from [cloud] in the config file. A provider's table, such as [cloud.coder], replaces either.
# $agent is the agent's product name, such as Claude Code.
settings-cloud-setup-install = Install { $agent }
settings-cloud-setup-none = No setup script of your own.
# Under the first lines of the user's script, how many more it has.
settings-cloud-setup-more = { $count ->
        [one] … { $count } more line
       *[other] … { $count } more lines
    }
settings-cloud-setup-edit = Edit setup script in config file

## Images: each project's sandbox image at a provider.

settings-cloud-images = Images
# $provider is the provider's config key, which picks what it calls a machine.
settings-cloud-images-note = { $provider ->
        [daytona] Each project's image, built from its .herdr/sandbox/Dockerfile or dev container with Herdr added. A new sandbox for a project starts from its image, and a changed recipe builds a new one. Build ahead from a workspace's menu with Build sandbox images.
       *[other] Each project's image, built from its .herdr/sandbox/Dockerfile or dev container with Herdr added. A new container for a project starts from its image, and a changed recipe builds a new one. Build ahead from a workspace's menu with Build sandbox images.
    }
# $project is the project's name, $path its folder.
settings-cloud-image-build-error = { $project }: { $error } Open { $path } in a window to build its image.
settings-cloud-image-no-window = Open a main Herdr window to build images.
# $image is the image's tag; $built when it was built, a settings-cloud-ago- message.
settings-cloud-image-ready = Ready · { $image } · built { $built }
settings-cloud-image-failed = Build failed { $built }
# A build's progress; $status is the builder's own output.
settings-cloud-image-step = Step { $step } of { $total }: { $status }
settings-cloud-image-log = Log
settings-cloud-image-hide-log = Hide log
settings-cloud-image-build = Build now
settings-cloud-image-reset = Reset
settings-cloud-image-delete = Delete image
settings-cloud-image-reset-confirm = Deletes the image here; the next new machine for this project builds it again. Click Delete image to confirm.
settings-cloud-ago-now = just now
settings-cloud-ago-minutes = { $count } min ago
settings-cloud-ago-hours = { $count } h ago
settings-cloud-ago-days = { $count ->
        [one] { $count } day ago
       *[other] { $count } days ago
    }
