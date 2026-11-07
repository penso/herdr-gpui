use super::*;
use std::sync::Arc;

#[test]
fn busy_cache_keeps_observations_but_not_across_credentials_or_signout() {
    let _serial = SERIAL.lock().unwrap();
    let mut auth = crate::Auth::connected_fixture();
    let token = Arc::new(SecretString::from("quota-contention"));
    auth.profile.as_mut().unwrap().token = token.clone();
    record(&token, &headers());
    auth.poll();
    let expected = auth.quota().unwrap();
    {
        let _writer = QUOTAS.lock().unwrap();
        assert_eq!(latest(&token), Observation::Unavailable);
        auth.poll();
        assert_eq!(auth.quota(), Some(expected));

        auth.profile.as_mut().unwrap().token = Arc::new("quota-replacement".into());
        assert_eq!(
            auth.quota(),
            None,
            "a changed credential cannot expose old data before polling"
        );
        auth.poll();
        assert_eq!(
            auth.quota(),
            None,
            "a busy cache cannot carry another credential's quota"
        );
    }
    auth.profile.as_mut().unwrap().token = token;
    auth.poll();
    assert_eq!(auth.quota(), Some(expected));
    let _writer = QUOTAS.lock().unwrap();
    auth.profile = None;
    assert_eq!(auth.quota(), None);
    auth.poll();
    assert_eq!(auth.quota(), None);
}

#[test]
fn a_readable_cache_can_report_that_an_observation_was_evicted() {
    let _serial = SERIAL.lock().unwrap();
    let mut auth = crate::Auth::connected_fixture();
    let token = Arc::new(SecretString::from("quota-evicted"));
    auth.profile.as_mut().unwrap().token = token.clone();
    record(&token, &headers());
    auth.poll();
    assert!(auth.quota().is_some());
    QUOTAS
        .lock()
        .unwrap()
        .retain(|(stored, _)| *stored != key(&token));
    assert_eq!(latest(&token), Observation::Ready(None));
    auth.poll();
    assert_eq!(auth.quota(), None);
}
