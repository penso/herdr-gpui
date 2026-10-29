# Settings > Cloud devices > Sprites, Fly's persistent machines. Sprite
# names stay as they are.

settings-cloud-sprites-note = Create Fly sprites and use them as devices: persistent Linux machines that sleep when idle and wake on the next command. Herdr reaches each one through the API's exec stream, relayed by this app; the token never leaves this computer.
settings-cloud-sprites-not-configured = Not configured. Check the API URL, add a token, then save.
settings-cloud-sprites-count = { $count ->
        [one] Connected. The token sees { $count } sprite made here.
       *[other] Connected. The token sees { $count } sprites made here.
    }
settings-cloud-sprites-api-url = API URL
settings-cloud-sprites-token = API token
settings-cloud-sprites-token-placeholder = Sprites API token
settings-cloud-sprites-token-none = No API token saved.
settings-cloud-sprites-token-unusable = The API token could not be used: { $error }
settings-cloud-sprites-token-environment = Set by HERDR_SPRITES_TOKEN, which takes precedence.
settings-cloud-sprites-token-saved = Saved. Type a new token to replace it.
# `sprite org auth` is a command; keep it as it is.
settings-cloud-sprites-token-missing = Not saved yet. Create one with `sprite org auth` or in the Sprites dashboard.
settings-cloud-sprites-forget-token = Forget token

## New sprite and Sprites devices.

settings-cloud-sprites-new = New sprite
settings-cloud-sprites-new-note = Creates a sprite with a generated name, runs Herdr's server there as a service so it is back after every sleep, and adds it as a device.
settings-cloud-sprites-create = Create sprite
settings-cloud-sprites-no-window = Open a main Herdr window to create a sprite.
settings-cloud-sprites-devices = Sprites devices
settings-cloud-sprites-devices-note = Remove forgets a sprite and leaves it at Fly. To delete one, use Remove device… in its sidebar menu, which first checks it for unpushed work.
