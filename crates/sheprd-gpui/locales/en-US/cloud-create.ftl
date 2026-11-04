# Asking before creating the cloud machines a command asked for.
# $provider is a cloud provider, such as Coder; $kind is what it calls one
# machine, in English: machine, sandbox, workspace, container, VM, or sprite.

cloud-create-title = { $count ->
        [one] Create { $count } { $provider } { $kind }?
       *[other] Create { $count } { $provider } { $kind ->
            [machine] machines
            [sandbox] sandboxes
            [workspace] workspaces
            [container] containers
            [VM] VMs
            [sprite] sprites
           *[other] { $kind }s
        }?
    }
# A Coder workspace's template and preset, each named or the default.
cloud-create-template-preset = From template { $template }, with preset { $preset }.
cloud-create-template-default-preset = From template { $template }, with its default preset.
cloud-create-only-template-preset = From the deployment's only template, with preset { $preset }.
cloud-create-only-template-default-preset = From the deployment's only template, with its default preset.
cloud-create-install = Herdr is installed on each one that lacks it.
cloud-create-no-install = Herdr is not installed; one that lacks it fails.
cloud-create-setup = The [cloud] setup script runs on each before it is ready.
cloud-create-no-setup = No setup script runs.
cloud-create-cost = A command asked for this. { $provider } bills each one until it is destroyed.
cloud-create-cancel = Cancel
cloud-create-submit = Create { $count }
# The notification once the batch ends.
cloud-create-done-title = Cloud devices created
cloud-create-done = { $total ->
        [one] { $ready } of { $total } new cloud device is ready. Choose one in the device picker.
       *[other] { $ready } of { $total } new cloud devices are ready. Choose one in the device picker.
    }
# $device is the first failed device's name.
cloud-create-done-failed = { $total ->
        [one] { $ready } of { $total } new cloud device is ready. Choose one in the device picker. { $failed } failed. { $device }: { $error }
       *[other] { $ready } of { $total } new cloud devices are ready. Choose one in the device picker. { $failed } failed. { $device }: { $error }
    }
cloud-create-unknown-error = unknown error
