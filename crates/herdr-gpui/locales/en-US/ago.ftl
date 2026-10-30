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
