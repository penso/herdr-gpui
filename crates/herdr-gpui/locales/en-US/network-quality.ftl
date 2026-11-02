# The status bar's network reading, NET 31ms 0%, and the tooltip over it.

# Short label before the reading; keep it about as short as this.
network-quality-label = NET
# $ms is a whole number of milliseconds.
network-quality-latency = { $ms }ms
network-quality-offline = offline
# What a screen reader says; $loss is a whole percentage.
network-quality-spoken = Network { $ms } ms
network-quality-spoken-loss = Network { $ms } ms, { $loss }% loss
network-quality-spoken-offline = Network offline
# $target is the address probed, $seconds how often.
network-quality-target = TCP connect to { $target } from this machine every { $seconds } s
network-quality-last = Last: { $ms } ms
# $reason is the system's words for why the connection failed.
network-quality-last-lost = Last: no connection ({ $reason })
network-quality-median = Median { $ms } ms of the last { $probes } probes
# $minutes is how far back the readings go at most.
network-quality-range = Range { $min }–{ $max } ms over up to { $minutes } min
network-quality-loss =
    { $probes ->
        [one] Loss { $loss }% of { $probes } probe over up to { $minutes } min
       *[other] Loss { $loss }% of { $probes } probes over up to { $minutes } min
    }
