# Plan usage: the status bar's segments, the panel a segment opens, and the
# activity bar's Usage section. Service, account, plan, and host names are
# shown as reported.

usage-section-title = Usage
usage-refresh = Refresh
usage-updating = Updating…
usage-off = Plan usage is off. Turn on Usage in Settings to read your agents' limits.
usage-not-read-yet = Not read yet
usage-read-while-selected = Read while this host is selected
usage-no-agents = No signed-in agents found on this host.
usage-not-updated-yet = Not updated yet
# $time is a countdown such as "2h 53m".
usage-updated-ago = Updated { $time } ago
usage-updated-just-now = Updated less than a minute ago
# The panel's line under the service's name: $updated is one of the
# usage-updated messages above, $host the host the numbers are from.
usage-panel-subtitle = { $updated } · { $host }

# Status bar.
usage-unavailable = Usage unavailable
usage-refresh-usage = Refresh usage
# What a screen reader says for a service's segment.
usage-segment = { $name } usage { $percent }% used
usage-segment-idle = { $name } usage
# A segment's window: $percent used, and $time until it resets.
usage-bar-resets = { $percent }% used { $time }
# A window the service names itself, such as a model's quota.
usage-bar-named = { $percent }% used { $name }
# A window whose reset is unknown, by its length, abbreviated.
usage-bar-kind = { $percent }% used { $kind ->
        [session] 5h
        [daily] day
        [weekly] wk
       *[monthly] mo
    }

# Countdowns, abbreviated: 47m, 2h 53m, 4d 11h.
usage-countdown-minutes = { $minutes }m
usage-countdown-hours = { $hours }h { $minutes }m
usage-countdown-days = { $days }d { $hours }h

# A limit's name; $name is a window the service names itself.
usage-kind = { $kind ->
        [session] Session
        [daily] Daily
        [weekly] Weekly
        [monthly] Monthly
       *[named] { $name }
    }
usage-limit-left = { $kind ->
        [session] Session
        [daily] Daily
        [weekly] Weekly
        [monthly] Monthly
       *[named] { $name }
    } { $left }% left
usage-left = { $left }% left
usage-resets-in = Resets in { $time }
usage-pace-ran-out = Ran out
# Whether spending is ahead of an even pace ($margin percent), then whether
# what is left lasts until the window resets or runs out in $time.
usage-pace = { $standing ->
        [reserve] { $margin }% in reserve
        [deficit] { $margin }% in deficit
       *[even] On pace
    } · { $outlook ->
        [runs-out] Runs out in { $time }
       *[lasts] Lasts until reset
    }
# $amount and $total are amounts with their unit, such as $12.30.
usage-balance-of = { $amount } of { $total }

# The panel.
usage-dashboard = Usage Dashboard
usage-status-page = Status Page
usage-daily-spend = Daily spend
usage-other-accounts = Other accounts
usage-account-disabled = disabled
# Why another claude-swap account's numbers are missing or old, and
# whether claude-swap holds it out of rotation ($disabled is yes or no).
usage-account-note = { $status ->
        [stale] last known
        [token-expired] token expired
        [api-key] API key
        [keychain] keychain unavailable
        [relogin] re-login needed
        [foreign] foreign credential
        [no-credentials] no credentials
       *[unavailable] unavailable
    }{ $disabled ->
        [yes] , disabled
       *[no] {""}
    }
usage-keychain-needed = { $name }'s sign-in is kept in the macOS Keychain, so reading it makes macOS ask for permission. Choose Always Allow to see these numbers at launch without being asked again.
usage-keychain-allow = Allow Keychain Access
usage-keychain-denied = macOS was not allowed to share { $name }'s sign-in. Try again to have macOS ask once more.
usage-keychain-try-again = Try Again
usage-setup = Set up
# $setting is a config key such as [usage.providers.zai] api_key, and
# $variables the environment variables that also set it.
usage-setup-setting-or-env = { $setting } or { $variables }
