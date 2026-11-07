# Settings > Cloud devices > boxd. boxd is a product name and stays
# lowercase. Machine names stay as they are.

settings-cloud-boxd-note = Create boxd machines and use them as devices: full Linux microVMs with a persistent disk that start in about a second and sleep when idle. Sheprd reaches each one through the API's exec stream, relayed by this app; the API key never leaves this computer.
settings-cloud-boxd-not-configured = Not configured. Check the API URL, add an API key, then save.
settings-cloud-boxd-count = { $count ->
        [one] Connected. The API key sees { $count } machine made here.
       *[other] Connected. The API key sees { $count } machines made here.
    }
settings-cloud-boxd-api-url = API URL
settings-cloud-boxd-key = API key
settings-cloud-boxd-key-placeholder = boxd API key
settings-cloud-boxd-key-none = No API key saved.
settings-cloud-boxd-key-unusable = The API key could not be used: { $error }
settings-cloud-boxd-key-environment = Set by HERDR_BOXD_API_KEY, which takes precedence.
settings-cloud-boxd-key-saved = Saved. Type a new key to replace it.
# `boxd auth keys create` is a command; keep it as it is.
settings-cloud-boxd-key-missing = Not saved yet. Create one in the boxd console or with `boxd auth keys create`.
settings-cloud-boxd-forget-key = Forget API key

## New machine and boxd devices.

settings-cloud-boxd-new = New machine
settings-cloud-boxd-new-note = Creates a boxd machine with a generated name, installs Herdr on approval, and adds it as a device. A machine in standby or hibernated wakes when Sheprd connects.
settings-cloud-boxd-create = Create machine
settings-cloud-boxd-no-window = Open a main Sheprd window to create a boxd machine.
settings-cloud-boxd-devices = boxd devices
settings-cloud-boxd-devices-note = Remove forgets a machine and leaves it at boxd. To delete one, use Remove device… in its sidebar menu, which first checks it for unpushed work.
