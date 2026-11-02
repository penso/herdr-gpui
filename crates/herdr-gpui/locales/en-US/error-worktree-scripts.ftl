# Why a worktree script could not be read or started.
# .herdr/worktree.toml is the file the scripts are read from.

error-worktree-scripts-busy = Another worktree script is still starting
error-worktree-scripts-checkout = The daemon did not identify this workspace's checkout
error-worktree-scripts-encoding = .herdr/worktree.toml is not valid UTF-8
error-worktree-scripts-not-git = This workspace is not a Git checkout Herdr knows yet
error-worktree-scripts-nul = .herdr/worktree.toml scripts must not contain NUL bytes
# $error is the TOML parser's message.
error-worktree-scripts-parse = .herdr/worktree.toml: { $error }
error-worktree-scripts-read = Could not read .herdr/worktree.toml: { $error }
error-worktree-scripts-remote = Could not read .herdr/worktree.toml on the host: { $error }
error-worktree-scripts-response = Unexpected daemon response while opening the script's tab
# $limit is a number of bytes.
error-worktree-scripts-size = .herdr/worktree.toml exceeds { $limit } bytes
error-worktree-scripts-unsupported-host = Worktree scripts cannot be read from a WSL distribution yet
