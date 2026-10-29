# Settings > Cloud devices > Boat. Sandbox, machine, and organization
# names come from Boat and stay as they are.

settings-cloud-boat-note = Create Boat sandboxes and use them as devices. Herdr authorizes a GUI-owned SSH key and pins each machine's host key from Boat's authenticated API.
settings-cloud-boat-not-configured = Not configured. Check the API URL, add an API key, then save.
settings-cloud-boat-sandboxes = { $count ->
        [one] Connected. The key sees { $count } sandbox.
       *[other] Connected. The key sees { $count } sandboxes.
    }
settings-cloud-boat-sandboxes-at-least = Connected. The key sees { $count } or more sandboxes.
settings-cloud-boat-key-environment = Set by HERDR_BOAT_API_KEY, which takes precedence.
settings-cloud-boat-key-missing = Not saved yet. Create one in the Boat dashboard under API Keys.

## The account's fields, each a label and its placeholder.

settings-cloud-boat-api-url = API URL
settings-cloud-boat-organization = Organization
settings-cloud-boat-organization-placeholder = Optional; the active personal or organization wallet if empty
settings-cloud-boat-machine-type = Machine type
# The values are what the field takes; keep them as they are.
settings-cloud-boat-machine-type-placeholder = Optional: small, default, large, or xlarge
settings-cloud-boat-environment = Environment
settings-cloud-boat-environment-placeholder = Optional; the account's default environment if empty
settings-cloud-boat-auto-stop = Auto-stop after
# "never" is what the field takes; keep it as it is.
settings-cloud-boat-auto-stop-placeholder = never, or 1–2592000 seconds
settings-cloud-boat-auto-stop-note = Use never to disable auto-stop: billing continues until you stop the sandbox in Boat. A number sets a fixed deadline, even during SSH work (trials require 1–7200 seconds). Applies on create and resume. For a running device, save then use Finish setup to extend from now or disable auto-stop.

## New sandbox and Boat devices.

settings-cloud-boat-new-note = Creates a sandbox with a generated name. Its device is saved before setup so an interruption can be retried without creating another sandbox.
settings-cloud-boat-recover = Recover creation
# $name is the sandbox's generated name.
settings-cloud-boat-pending = Pending creation: { $name }. Recover creation reuses the original request and options, even after restarting. Recovery expires after 23 hours; it never silently creates a replacement.
settings-cloud-boat-forget-pending = Forget pending request…
settings-cloud-boat-forget-pending-confirm = I checked Boat; forget request
settings-cloud-boat-forget-pending-warning = Check the Boat dashboard first: the original sandbox may exist and still be billed. Forgetting only removes this local request. Creating again can create a second sandbox.
settings-cloud-boat-devices = Boat devices
settings-cloud-boat-devices-note = Finish setup reuses the sandbox and applies Auto-stop after from now, extending a timed deadline or disabling it. The installation switch above also applies here; the setup script does not run again.
