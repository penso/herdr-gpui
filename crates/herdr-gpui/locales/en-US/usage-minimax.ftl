# MiniMax's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-minimax-setting-api-key = A MiniMax Coding Plan or Token Plan API key (sk-cp-…) from https://platform.minimax.io/user-center/basic-information/interface-key.
usage-minimax-setting-cookie = Used when no API key is set. Sign in to https://platform.minimax.io, open Developer Tools > Application > Cookies > https://platform.minimax.io, and copy every cookie (including HERTZ-SESSION) as "name=value; name2=value2".
usage-minimax-setting-region = "global" for minimax.io (the default) or "cn" for the China mainland platform at minimaxi.com.

## The report: the Coding or Token Plan's quota lanes, named by what each
## makes, and the points balance. Plan names, and the model names of lanes
## that make something else, stay as MiniMax sends them.

usage-minimax-points = Points
# What a quota lane makes.
usage-minimax-text = Text
usage-minimax-video = Video
usage-minimax-speech = Speech
usage-minimax-image-to-video = Image to video
usage-minimax-text-to-video = Text to video
usage-minimax-image = Image
usage-minimax-music = Music
# A lane's daily window; $service is what it makes, such as Video.
usage-minimax-daily = { $service } daily
# A lane's window of $hours hours, such as Speech 4h.
usage-minimax-hours = { $service } { $hours }h
