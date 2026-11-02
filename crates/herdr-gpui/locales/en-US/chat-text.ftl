# Durations and turn summaries in agent chats.

# How long something took, in the largest units: "950ms", "12s", "3m 4s",
# "1h 2m".
chat-text-milliseconds = { $ms }ms
chat-text-seconds = { $seconds }s
chat-text-minutes = { $minutes }m { $seconds }s
chat-text-hours = { $hours }h { $minutes }m

# How an agent's turn ended, after its answer. $duration is how long it
# took, such as "12s".
chat-text-turn-done = Worked { $duration }
chat-text-turn-cancelled = Cancelled after { $duration }
chat-text-turn-limit = Stopped at its limit after { $duration }
chat-text-turn-refused = Refused after { $duration }
chat-text-turn-failed = Failed after { $duration }
chat-text-turn-other = Ended after { $duration }
# The same with the tokens the turn used. $turn is one of the lines above;
# $input, $output, and $thought are counts such as "3.2k".
chat-text-turn-tokens = { $turn } · { $input } in · { $output } out
chat-text-turn-tokens-thinking = { $turn } · { $input } in · { $output } out · { $thought } thinking
