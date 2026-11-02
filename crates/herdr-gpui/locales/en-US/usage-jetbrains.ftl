# JetBrains AI's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-jetbrains-setting-ide-path = The config directory of the IDE to read, when the most recently used one is not the right one, e.g. "~/Library/Application Support/JetBrains/IntelliJIdea2025.3" on a Mac or "~/.config/JetBrains/PyCharm2025.3" on Linux. Its options/AIAssistantQuotaManager2.xml appears once AI Assistant has been used. On the Mac running Herdr the IDEs are only looked for when this is set or jetbrains is in show_providers, as macOS may ask before Herdr reads another app's data.

## The report: AI Assistant credits from the IDE's quota file. IDE names
## and the IDE's own quota word, such as Available, stay as written.

usage-jetbrains-credits-left = Credits left
# JetBrains' AI Assistant, whose credits these are.
usage-jetbrains-ai-assistant = AI Assistant
usage-jetbrains-ide = IDE
usage-jetbrains-quota = Quota
usage-jetbrains-refill = Refill
# The credits the next refill adds: $amount is a count as written, such as
# 1,000,000; $count is the same number, for the plural.
usage-jetbrains-refill-amount = { $count ->
       *[other] { $amount } credits
    }
# The same, with how often it comes: $days is a number of days, such as 30.
usage-jetbrains-refill-every = { $count ->
       *[other] { $amount } credits
    } every { $days ->
       *[other] { $days } days
    }
