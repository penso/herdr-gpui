use super::*;
use crate::http::split_aliases;
use chrono::Utc;
use serde_json::json;

#[test]
fn both_github_rate_limit_spellings_pause_the_entire_batch() {
    for kind in ["RATE_LIMIT", "RATE_LIMITED"] {
        let now = Utc::now();
        let reset = now + chrono::TimeDelta::minutes(29);
        let errors = json!([{"type": kind, "message": "Quota exhausted", "path": ["p0"]}]);
        assert!(
            matches!(query_error(&errors, None, "github_app_user", Some(reset)), Error::RateLimit { reset: Some(at) } if at == reset)
        );
        let mut cooldown = None;
        let result = split_aliases(
            json!({"errors": errors}),
            &[("p0".into(), "owner/repo".into())],
            "github_app_user",
            Some(reset),
            now,
            &mut cooldown,
        );
        assert!(matches!(result, Err(Error::RateLimit { .. })));
        assert_eq!(cooldown, Some(Duration::from_secs(29 * 60)));
    }
}
