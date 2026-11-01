# Deepgram's plan usage, in its usage panel.

## Setup: how to find each value the provider reads. Setting names,
## environment variables, URLs, cookie and header names, and the
## service's own menu paths stay as written.

usage-deepgram-setting-api-key = A Deepgram API key with the usage:read scope, created in the Deepgram Console (https://console.deepgram.com) under API Keys.
usage-deepgram-setting-project-id = Optional project UUID from the Deepgram Console. Leave unset to sum usage over every project the key can see.
usage-deepgram-setting-base-url = Optional HTTPS API URL for a proxy. Defaults to https://api.deepgram.com/v1.

## The report: usage summed over the key's projects. Project names stay as
## Deepgram sends them.

usage-deepgram-no-projects = No projects
# The plan line for a key that sees one project; $project is its name.
usage-deepgram-project = Project: { $project }
# The plan line for a key that sees several projects; $count counts them.
usage-deepgram-projects = { $count ->
       *[other] { $count } projects
    }
usage-deepgram-usage-summary = Usage summary
usage-deepgram-requests = Requests
usage-deepgram-audio = Audio
# $hours is the audio transcribed, in hours, such as 1.8; $billable is the
# hours billed, such as 2.5.
usage-deepgram-audio-hours = { $hours } hours · { $billable } billable
usage-deepgram-agent-hours = Agent hours
usage-deepgram-tokens = Tokens
usage-deepgram-tts-characters = TTS characters
usage-deepgram-period = Period
# $start and $end are dates, such as 2026-05-20 and 2026-06-30.
usage-deepgram-period-range = { $start } to { $end }
