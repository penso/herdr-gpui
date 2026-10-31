# An issue's or pull request's page in the orchestrator.

# The facts line under an item's title, joined with " · ". $author is a
# login; $ago is how long ago, such as 5 min ago or yesterday.
orchestrator-detail-opened-by = { $author } opened { $ago }
orchestrator-detail-opened = opened { $ago }
orchestrator-detail-updated = updated { $ago }
# A pull request's size: lines added and removed, and files changed.
orchestrator-detail-size = +{ $additions } −{ $deletions } in { $files ->
        [one] { $files } file
       *[other] { $files } files
    }
