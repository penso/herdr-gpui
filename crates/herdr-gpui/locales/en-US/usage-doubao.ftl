# Doubao's plan usage, in its usage panel.

## The report: the Coding Plan's and Agent Plan's windows. The sign-in
## method, such as sso, stays as arkcli reports it.

# A window of a plan other than the personal Coding Plan: $plan is agent
# (the Agent Plan), team (the team Coding Plan), or agent-team (the team
# Agent Plan); $period is session (five hours), weekly, or monthly.
usage-doubao-window = { $plan ->
        [agent] Agent
        [team] Team
       *[agent-team] Agent team
    } { $period ->
        [session] 5-hour
        [weekly] weekly
       *[monthly] monthly
    }
