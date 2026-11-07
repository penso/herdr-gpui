//! Refresh reads everything again, checks included: a re-run check moves
//! neither the pull request's head nor its update time, so only Refresh or
//! the five-minute interval would read its new result.
use super::*;

#[test]
fn refresh_forgets_when_checks_were_last_read() {
    let (_service, mut worker, _) = Service::prepare(Request {
        target: ConnectTarget::Local,
        checkout: "/nowhere".into(),
        workspace_id: "w1".into(),
        token: None,
        data_root: None,
        azure_store: Default::default(),
        gitlab_store: Default::default(),
        timing: Timing::default(),
    });
    let key = SourceKey {
        provider: Provider::Github,
        host: "github.com".into(),
        repository: "penso/herdr-gpui".into(),
    };
    worker.checks.insert(
        key.clone(),
        schedule::ChecksRead {
            at: Instant::now(),
            open: Vec::new(),
        },
    );
    worker
        .retry
        .push((key, Instant::now() + Duration::from_secs(300)));
    worker.refreshed(None);
    assert!(worker.checks.is_empty(), "the next sync reads checks");
    assert!(worker.retry.is_empty(), "the next sync reads every source");
}
