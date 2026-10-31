# How long ago something happened, after a label or inside a sentence:
# "Last seen 3 h ago", "Restore it from 2 days ago?". $count is whole
# minutes, hours, or days.

ago-now = just now
ago-minutes = { $count } min ago
ago-hours = { $count } h ago
ago-days = { $count ->
    [1] yesterday
    [one] { $count } day ago
   *[other] { $count } days ago
}

# A compact age in a list column, where a sentence has no room: "now",
# "12m", "5h", "3d", "2w". $count is whole minutes, hours, days, or weeks.

ago-short-now = now
ago-short-minutes = { $count }m
ago-short-hours = { $count }h
ago-short-days = { $count }d
ago-short-weeks = { $count }w
