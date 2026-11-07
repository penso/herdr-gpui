# The Pull requests list's State menu and its Projects menu, which browses an
# Azure DevOps organization's projects and repositories without a checkout.
# Organization, project, and repository names are shown as they are.

# The State menu's choices; the control shows the one picked.
orchestrator-browse-state-open = Open
orchestrator-browse-state-all = All states
orchestrator-browse-state-draft = Draft
orchestrator-browse-state-merged = Merged
# Closed without merging; Azure DevOps calls it abandoned.
orchestrator-browse-state-closed = Closed or abandoned

# The control that opens the Projects menu.
orchestrator-browse-projects = Browse
# A menu row's button that reads the organization's projects again.
orchestrator-browse-projects-refresh = Read { $organization }'s projects again
orchestrator-browse-projects-loading = Reading projects…
orchestrator-browse-projects-empty = No repositories
# Under a long list, the rows it leaves out.
orchestrator-browse-projects-more = { $count } more repositories not shown
# Under the list once as many repositories are listed as one view keeps.
orchestrator-browse-projects-full = { $count } repositories are listed; remove one to add another
