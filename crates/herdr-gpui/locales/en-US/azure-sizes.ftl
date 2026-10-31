# Azure VM and sandbox sizes in Settings > Cloud devices. vCPU is a count of
# virtual processors; GiB is binary gigabytes. Size names such as
# Standard_D2pds_v6 come from Azure and stay as they are.

# $gib is a memory size in GiB, with one decimal when it is not whole.
azure-sizes-gib = { $gib } GiB
# A VM size's specs: $memory is an azure-sizes-gib message, $arch an
# azure-sizes-arch one.
azure-sizes-vm = { $cores } vCPU · { $memory } · { $arch }
# The processor architecture.
azure-sizes-arch = { $arch ->
        [arm] Arm
       *[x64] x64
    }
# A sandbox size: $cores may be a fraction, such as 0.5.
azure-sizes-sandbox = { $cores } vCPU · { $memory }
# A list price in US dollars; $amount already has its decimals.
azure-sizes-hourly = ${ $amount }/hr
# $monthly is the price over 730 running hours, in whole dollars.
azure-sizes-hourly-monthly = ${ $amount }/hr · ~${ $monthly }/mo
# A saved VM's size: $size is Azure's name for it, $spec an azure-sizes-vm
# message; Spot VMs run on spare capacity at a lower price.
azure-sizes-device-vm = { $size } · { $spec } · { $spot ->
        [yes] Spot
       *[no] regular price
    }
# A saved VM whose size the region does not list.
azure-sizes-device-vm-unlisted = { $size } · { $spot ->
        [yes] Spot
       *[no] regular price
    }
