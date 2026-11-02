# Building a project's sandbox image ahead of time, from its
# .herdr/sandbox/Dockerfile or dev container. $provider is the provider's
# name; $checkout names the project's checkout.

cloud-image-ready = { $provider } image ready
# $noun is a key naming what the provider calls its machines; translate
# each variant, not the keys.
cloud-image-ready-detail = New { $noun ->
        [VM] VMs
        [container] containers
        [sandbox] sandboxes
        [sprite] sprites
        [workspace] workspaces
       *[machine] machines
    } for { $checkout } start from it.
cloud-image-no-recipe = No { $provider } image to build
cloud-image-no-recipe-detail = { $checkout } has no .herdr/sandbox/Dockerfile or dev container to build from.
cloud-image-failed = { $provider } image build failed
# $error says why.
cloud-image-failed-detail = { $checkout }: { $error }
