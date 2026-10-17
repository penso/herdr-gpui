//! A device counts as being removed while the sidebar removes it or its
//! machine is deleted at Azure: its header pulses, and a drop meanwhile says
//! so instead of offering to reconnect.
use super::fixture_window;
use crate::cloud::CloudProvider;

#[gpui::test]
fn a_device_being_deleted_at_azure_reads_as_removing(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "azure:herdr-gone".into(),
            "herdr-gone".into(),
            herdr_client::ConnectTarget::Cloud {
                provider: CloudProvider::Azure,
                account: "00000000-0000-0000-0000-000000000000".into(),
                id: "herdr-gone".into(),
                machine: "herdr-gone".into(),
                session: "default".into(),
            },
            true,
        ));
        let endpoint = view.endpoints.last().unwrap();
        assert!(!view.device_removing(endpoint));
        crate::azure::destroy::mark_for_test("herdr-gone", || {
            assert!(
                view.device_removing(endpoint),
                "deleted from Settings or auto-stop"
            );
        });
        assert!(!view.device_removing(endpoint));
    });
    view.update(cx, |view, _| {
        view.menu.removing_devices.insert("azure:herdr-gone".into());
        let endpoint = view.endpoints.last().unwrap();
        assert!(view.device_removing(endpoint), "removed from the sidebar");
    });
}
