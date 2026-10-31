# Asking before destroying the saved cloud devices a command named.
# $provider is a cloud provider, such as Azure; $machine is what it calls one
# machine, such as VM or workspace; $leftovers is what else the provider
# deletes with it, as the provider words it.

cloud-destroy-failed = Could not destroy { $failed } of { $total ->
        [one] { $total } cloud device
       *[other] { $total } cloud devices
    }. { $device }: { $error }
cloud-destroy-unknown-error = unknown error
cloud-destroy-title = { $count ->
        [one] Destroy { $count } device?
       *[other] Destroy { $count } devices?
    }
cloud-destroy-note = A command asked for this. Their files are not kept, and this cannot be undone.
cloud-destroy-cancel = Cancel
cloud-destroy-submit = { $count ->
        [one] Destroy { $count } device
       *[other] Destroy { $count } devices
    }
cloud-destroy-delete = Delete the { $machine } at { $provider }{ $leftovers } and remove it from Herdr
cloud-destroy-forget = Remove it from Herdr only; { $provider } cannot delete its { $machine } from here
