# Settings > Cloud devices > Azure. Size, region, group, and machine names
# come from Azure and the user and stay as they are. $kind is `vm` or
# `sandbox`, for what kind of machine a device is.

settings-cloud-azure-note = VMs and Container Apps sandboxes in one Azure subscription, used as devices. VMs are reached over SSH; sandboxes through the Azure data plane.

## The account's fields, each a label and its placeholder.

settings-cloud-azure-subscription = Subscription ID
settings-cloud-azure-subscription-placeholder = Azure subscription UUID
settings-cloud-azure-group = Resource Group
settings-cloud-azure-group-placeholder = Default: herdr-devices
settings-cloud-azure-location = Location
settings-cloud-azure-location-placeholder = Default: centralus
settings-cloud-azure-vm-size = VM Size
settings-cloud-azure-vm-size-placeholder = Default: Standard_D2plds_v6
settings-cloud-azure-auto-stop = Auto-stop
# "never" and the times are what the field takes; keep them as they are.
settings-cloud-azure-auto-stop-placeholder = never, or idle time before an idle VM is deleted (e.g. 30m, 1h, 2h)
settings-cloud-azure-tenant = Tenant ID
settings-cloud-azure-tenant-placeholder = Optional Service Principal tenant UUID
settings-cloud-azure-sandbox-group = Sandbox Group
settings-cloud-azure-sandbox-group-placeholder = Default: herdr-sandboxes
settings-cloud-azure-sandbox-region = Sandbox Region
settings-cloud-azure-sandbox-region-placeholder = Default: same as Location
settings-cloud-azure-sandbox-image = Sandbox Image
settings-cloud-azure-sandbox-image-placeholder = Public disk image. Default: ubuntu
settings-cloud-azure-client-id = Client ID
settings-cloud-azure-client-id-placeholder = Optional Service Principal application UUID
settings-cloud-azure-secret = Client Secret
settings-cloud-azure-secret-placeholder = Client secret (masked)

## How the account signs in.

settings-cloud-azure-cli-detected = Azure CLI (`az`) detected. Enter Subscription ID or click Save to use active CLI account.
settings-cloud-azure-not-configured = Not configured. Enter a Subscription ID and credentials, then Save.
settings-cloud-azure-signed-in-cli = Authenticated via active Azure CLI (`az`) account.
settings-cloud-azure-signed-in-secret = Authenticated via saved Service Principal client secret.
settings-cloud-azure-signed-in-environment = Authenticated via HERDR_AZURE_CLIENT_SECRET.
settings-cloud-azure-credential-missing = Subscription configured. Add a client secret or log into `az` CLI to connect.

## New VM and New sandbox.

settings-cloud-azure-new-vm = New VM
settings-cloud-azure-create-vm = Create VM
settings-cloud-azure-creating-vm = Creating Azure VM { $name }. The device picker shows its progress.
settings-cloud-azure-no-window-vm = Open a main Herdr window to create a VM.
settings-cloud-azure-new-sandbox = New sandbox
settings-cloud-azure-create-sandbox = Create sandbox
settings-cloud-azure-creating-sandbox = Creating Azure sandbox { $name }. The device picker shows its progress.
settings-cloud-azure-no-window-sandbox = Open a main Herdr window to create a sandbox.
settings-cloud-azure-vm-sizes-note = New VMs keep their OS disk on the VM's local storage, so there is no disk to pay for, and auto-stop deletes an idle one: keep work in Git. They run as Spot where Azure offers it to the subscription, and Azure may delete a Spot VM when it needs the capacity back; otherwise they are billed at the regular price. Linux list prices in { $location }, in US dollars, as of now: Spot prices move.
settings-cloud-azure-sizes-unknown = Specs and prices show once the account is saved and reachable.
settings-cloud-azure-size-no-local-disk = No local storage: a Standard SSD disk, billed until deleted
settings-cloud-azure-size-not-offered = Not offered in { $location }
# Hourly and monthly prices, as Azure lists them.
settings-cloud-azure-price-spot-regular = Spot { $spot } · regular { $regular }
settings-cloud-azure-price-spot = Spot { $spot }
settings-cloud-azure-price-regular = Regular { $regular }
settings-cloud-azure-size-other = Other
settings-cloud-azure-size-other-detail = Any size the region offers
settings-cloud-azure-size-use = Use size
settings-cloud-azure-sandbox-sizes-note = Billed per second at Container Apps rates in { $location } while running; a suspended sandbox is not billed for CPU or memory.
settings-cloud-azure-sandbox-sizes-unknown = Memory is 2 GiB and disk 20 GiB per vCPU.
# $size is the size's vCPUs and memory, $disk its disk in GiB, $price what
# it costs an hour.
settings-cloud-azure-sandbox-size = { $size } · { $disk } GiB disk
settings-cloud-azure-sandbox-price = { $size } · { $disk } GiB disk · { $price } while running
settings-cloud-azure-vcpu = { $cores } vCPU

## Azure devices.

settings-cloud-azure-devices = Azure devices
settings-cloud-azure-remove-note = Removing a device deletes it at Azure, with its disk and IP. Remove from Herdr only, offered beside it, leaves it at Azure, still billing.
settings-cloud-azure-device = { $kind ->
        [vm] VM in { $group }
       *[sandbox] sandbox in { $group }
    }
settings-cloud-azure-device-size = { $kind ->
        [vm] VM · { $size } in { $group }
       *[sandbox] sandbox · { $size } in { $group }
    }
settings-cloud-azure-delete = { $kind ->
        [vm] Delete VM
       *[sandbox] Delete sandbox
    }
settings-cloud-azure-delete-vm-warning = Delete removes the VM { $name } with its disk, public IP, and network at Azure as well as here. Its files are not kept, and this cannot be undone. Remove from Herdr only leaves it at Azure, still billing.
settings-cloud-azure-delete-sandbox-warning = Delete removes the sandbox { $name } at Azure as well as here. Its files are not kept, and this cannot be undone. Remove from Herdr only leaves it at Azure, still billing.
settings-cloud-azure-destroy-contacting = Contacting Azure…
settings-cloud-azure-destroy-vm = Deleting the VM and its disk…
settings-cloud-azure-destroy-leftovers = Deleting its network and IP…
settings-cloud-azure-destroy-sandbox = Deleting the sandbox…
