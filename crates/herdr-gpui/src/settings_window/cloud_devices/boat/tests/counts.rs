use super::super::{SandboxCount, sandbox_status};

#[test]
fn account_status_distinguishes_exact_counts_from_full_pages() {
    for (count, expected) in [
        (
            SandboxCount::Exact(0),
            "Connected. The key sees \u{2068}0\u{2069} sandboxes.",
        ),
        (
            SandboxCount::Exact(1),
            "Connected. The key sees \u{2068}1\u{2069} sandbox.",
        ),
        (
            SandboxCount::Exact(99),
            "Connected. The key sees \u{2068}99\u{2069} sandboxes.",
        ),
        (
            SandboxCount::AtLeast(100),
            "Connected. The key sees \u{2068}100\u{2069} or more sandboxes.",
        ),
    ] {
        assert_eq!(sandbox_status(count), expected);
    }
}
