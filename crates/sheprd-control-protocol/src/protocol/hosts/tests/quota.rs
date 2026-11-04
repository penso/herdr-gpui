use super::*;

#[test]
fn quota_wire_preserves_unknown_allowance_and_retry_delay() {
    let op = HostOp::PrQuota;
    assert_eq!(
        serde_json::to_value(&op).unwrap(),
        serde_json::json!({"op": "pr_quota"})
    );
    let response = Reply::PrQuota {
        accounts: vec![AccountQuota {
            login: "octocat".into(),
            quota: None,
            retry_after_seconds: Some(1740),
        }],
    };
    let value = serde_json::to_value(&response).unwrap();
    assert_eq!(value["accounts"][0]["quota"], serde_json::Value::Null);
    assert_eq!(value["accounts"][0]["retry_after_seconds"], 1740);
    assert_eq!(serde_json::from_value::<Reply>(value).unwrap(), response);
}
