# Settings > Cloud devices > OpenSandbox. Server URLs, image names, and
# sandbox names stay as they are.

settings-cloud-opensandbox-note = Create sandboxes on your own OpenSandbox server and use them as devices. Herdr reaches each one through its execd stream, relayed by this app; the API key never leaves this computer.
settings-cloud-opensandbox-not-configured = Not configured. Check the server URL, add its API key, then save.
settings-cloud-opensandbox-sandboxes = { $count ->
        [one] Connected. The server has { $count } sandbox made here.
       *[other] Connected. The server has { $count } sandboxes made here.
    }
settings-cloud-opensandbox-unusable = The server could not be used: { $error }
settings-cloud-opensandbox-key-placeholder = OpenSandbox API key
settings-cloud-opensandbox-key-environment = Set by HERDR_OPENSANDBOX_API_KEY, which takes precedence.
# `api_key` and sandbox.toml are the server's own names; keep them as they are.
settings-cloud-opensandbox-key-missing = Not saved yet. It is the server's `api_key` in its sandbox.toml.

## The server's fields; each shows its default as its placeholder.

settings-cloud-opensandbox-url = Server URL
settings-cloud-opensandbox-image = Image
settings-cloud-opensandbox-cpu = CPU
settings-cloud-opensandbox-memory = Memory

## New sandbox and OpenSandbox devices.

settings-cloud-opensandbox-new-note = Creates a sandbox with a generated name and no expiry, keeps Herdr's server running there, and adds it as a device.
settings-cloud-opensandbox-devices = OpenSandbox devices
settings-cloud-opensandbox-devices-note = Remove forgets a sandbox and leaves it on the server. To delete one, use Remove device… in its sidebar menu, which first checks it for unpushed work.
