# A device's load: the status bar's CPU, memory, and GPU readings, the
# gauges on host rows, and the tooltip over them.

# Short labels before each reading; keep them about as short as these.
system-load-cpu = CPU
system-load-memory = MEM
system-load-gpu = GPU
system-load-video-memory = VRAM
# The device never answered.
system-load-unavailable = CPU and memory unavailable
# Waiting for the device's first answer.
system-load-waiting = CPU and memory…

# The tooltip's first line names the device.
system-load-this-machine = This machine
# $distro is the WSL distribution's name.
system-load-wsl = WSL · { $distro }
# $name is the Azure virtual machine's name.
system-load-azure-vm = Azure VM · { $name }
system-load-azure-sandbox = Azure sandbox
# $percent is a whole number; $cores is 0 when the device did not say.
system-load-tooltip-cpu =
    { $cores ->
        [0] CPU { $percent }%
        [one] CPU { $percent }% of { $cores } core
       *[other] CPU { $percent }% of { $cores } cores
    }
# The first reading is still being taken.
system-load-tooltip-cpu-measuring =
    { $cores ->
        [0] CPU measuring…
        [one] CPU measuring… of { $cores } core
       *[other] CPU measuring… of { $cores } cores
    }
# Load averages over 1, 5, and 15 minutes.
system-load-tooltip-load = load { $one } { $five } { $fifteen }
# Two parts of one tooltip line.
system-load-tooltip-joined = { $first } · { $second }
# $used and $total are sizes, such as "6.8 GB".
system-load-tooltip-memory = Memory { $used } of { $total } ({ $percent }%)
system-load-tooltip-gpu =
    { $cards ->
        [one] GPU { $percent }%
       *[other] GPU { $percent }% across { $cards } cards
    }
# A GPU that reports its memory but not how busy it is.
system-load-tooltip-gpu-unmeasured =
    { $cards ->
        [one] GPU
       *[other] GPU across { $cards } cards
    }
system-load-tooltip-video-memory = Video memory { $used } of { $total } ({ $percent }%)
system-load-tooltip-disk = Disk { $free } free of { $total } ({ $percent }% used)
# $uptime is how long the device has been up, as system-load-uptime-* write it.
system-load-tooltip-up = Up { $uptime }

# Sizes in binary units, as Activity Monitor and free -h count them.
system-load-gigabytes = { $size } GB
system-load-terabytes = { $size } TB
# How long a device has been up, in its two largest units, short enough
# for a table column.
system-load-uptime-days = { $days }d { $hours }h
system-load-uptime-hours = { $hours }h { $minutes }m
system-load-uptime-minutes = { $minutes }m
