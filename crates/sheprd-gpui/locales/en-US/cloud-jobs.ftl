# A cloud machine being added: the notice when it is ready or failed.
# $provider is the provider's name. $noun is a key naming what the provider
# calls its machines; translate each variant, not the keys. $device is the
# machine's name.

cloud-jobs-ready = { $provider } { $noun ->
        [VM] VM
        [container] container
        [sandbox] sandbox
        [sprite] sprite
        [workspace] workspace
       *[machine] machine
    } ready
# $setup is "yes" when a setup script runs once it connects.
cloud-jobs-ready-detail =
    { $setup ->
        [yes] { $device } is ready. Choose it in the device picker; its setup script runs in a new tab when it connects.
       *[no] { $device } is ready. Choose it in the device picker to start working.
    }
cloud-jobs-failed = { $provider } { $noun ->
        [VM] VM
        [container] container
        [sandbox] sandbox
        [sprite] sprite
        [workspace] workspace
       *[machine] machine
    } setup failed
# $error says why.
cloud-jobs-failed-detail = { $device }: { $error }
