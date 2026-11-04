use super::super::job_script;
use crate::{boat::setup::Target, cloud::SavedDevice};

#[test]
fn only_a_new_sandbox_carries_the_setup_script() {
    let script = || Some("echo hi\n".to_owned());
    assert_eq!(
        job_script(&Target::Create("herdr-box".into()), script()),
        script()
    );
    let saved = SavedDevice {
        provider: crate::cloud::CloudProvider::Boat,
        id: "bx_23456789".into(),
        label: "Saved box".into(),
        account: crate::boat::DEFAULT_API_URL.into(),
        machine: "herdr-box".into(),
        session: "default".into(),
        enabled: true,
    };
    // Finish setup, used to extend auto-stop, does not run it again.
    assert_eq!(job_script(&Target::Saved(saved), script()), None);
}
