use super::*;

#[test]
fn machine_names_follow_each_providers_argument_contract() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    for &provider in CloudProvider::ALL {
        for name in ["-oProxyCommand=x", "a b"] {
            let mut saved = device("w");
            saved.provider = provider;
            saved.machine = name.into();
            #[cfg(feature = "boat")]
            let display_only = provider == CloudProvider::Boat;
            #[cfg(not(feature = "boat"))]
            let display_only = false;
            assert_eq!(
                write(&path, vec![Entry::Known(saved)]).is_ok(),
                display_only,
                "{provider:?}: {name:?}"
            );
        }
    }
}
