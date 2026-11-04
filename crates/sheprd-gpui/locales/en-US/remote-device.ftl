# A phone paired for remote control, under its name in Settings > Remote
# control and the title bar's remote menu. $ago is an ago- message, such as
# "3 h ago".
remote-device-line = { $seen ->
        [connected] Connected now
        [never] Never connected
       *[seen] Last seen { $ago }
    } · { $push ->
        [on] notifications on
       *[off] notifications off
    }
