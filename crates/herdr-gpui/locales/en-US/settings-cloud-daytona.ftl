# Settings > Cloud devices > Daytona. Sandbox, region, and snapshot names
# come from Daytona and stay as they are.

settings-cloud-daytona-note = Create Daytona sandboxes and use them as devices. Herdr reaches each one through Daytona's SSH gateway with a short-lived token; the API key never leaves this computer.
settings-cloud-daytona-not-configured = Not configured. Check the API URL, add an API key, then save.
settings-cloud-daytona-sandboxes = { $count ->
        [one] Connected. The key sees { $count } sandbox.
       *[other] Connected. The key sees { $count } sandboxes.
    }
settings-cloud-daytona-key-environment = Set by HERDR_DAYTONA_API_KEY, which takes precedence.
settings-cloud-daytona-key-missing = Not saved yet. Create one in the Daytona dashboard under API Keys.

## The account's fields, each a label and, when not a default value, its placeholder.

settings-cloud-daytona-api-url = API URL
settings-cloud-daytona-organization = Organization ID
settings-cloud-daytona-organization-placeholder = Optional; the key's default organization if empty
settings-cloud-daytona-region = Region
settings-cloud-daytona-region-placeholder = Optional; Daytona's default region if empty
settings-cloud-daytona-snapshot = Snapshot
settings-cloud-daytona-snapshot-placeholder = Optional; Daytona's default snapshot if empty

## New sandbox and Daytona devices.

settings-cloud-daytona-new-note = Creates a sandbox with a generated name and adds it as a device once it is ready.
settings-cloud-daytona-devices = Daytona devices
