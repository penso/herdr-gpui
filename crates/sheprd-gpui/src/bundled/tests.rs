#![allow(clippy::unwrap_used)]

use super::*;

mod branding;
mod precedence;

fn helper(path: &Path) {
    std::fs::write(path, b"helper").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
