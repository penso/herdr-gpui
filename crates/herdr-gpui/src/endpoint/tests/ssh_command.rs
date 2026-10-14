use super::*;

#[gpui::test]
fn changing_the_command_retires_only_that_host_and_preserves_its_chosen_session(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update(cx, |view, cx| {
    let profile = "0123456789abcdef0123456789abcdef";
    let saved = host(profile, true);
    view.reconcile_catalog(vec![saved.clone(), host("other", true)], Vec::new(), cx);
    let id = format!("ssh:{profile}");
    assert!(view.select_endpoint(&id, cx));
    view.select_device_session(&id, "work", cx);
    let old_inbox = view.endpoints[1].connection.inbox.clone();
    let other_inbox = view.endpoints[2].connection.inbox.clone();
    let generation = view.endpoints[1].generation;
    view.config.devices.insert(
      profile.to_owned(),
      crate::config::DeviceSettings {
        ssh_command: herdr_client::SshCommand::try_from("tailscale ssh").unwrap(),
        ..Default::default()
      },
    );
    view.reload_ssh_commands(cx);
    assert_eq!(view.selected_endpoint, 1);
    assert!(matches!(
        &view.endpoints[1].connection.target,
        ConnectTarget::Ssh { target, session }
            if session == "work" && target.command.as_str() == "tailscale ssh"
    ));
    assert_eq!(view.endpoints[1].saved_ssh().unwrap().1, "default");
    assert!(view.endpoints[1].generation > generation);
    assert!(!Arc::ptr_eq(
      &old_inbox,
      &view.endpoints[1].connection.inbox
    ));
    assert!(Arc::ptr_eq(
      &other_inbox,
      &view.endpoints[2].connection.inbox
    ));
    // A delayed snapshot remains in the retired generation's mailbox.
    match old_inbox.lock() {
      Ok(mut inbox) => inbox.apply(ClientEvent::Snapshot(Arc::new(
        crate::sidebar::layout_tests::snapshot(99),
      ))),
      Err(error) => panic!("retired inbox poisoned: {error}"),
    }
    view.endpoints[1].poll(Instant::now());
    assert!(view.endpoints[1].live.snapshot.is_none());
    view.reconcile_catalog(vec![saved, host("other", true)], Vec::new(), cx);
    assert_eq!(view.selected_endpoint, 1);
    assert_eq!(
      view.endpoints[1].connection.target.remote_session(),
      Some("work")
    );
    // Clearing the preference is another transport transition, not a
    // session change or an edit to the upstream catalog.
    view.config.devices.remove(profile);
    view.reload_ssh_commands(cx);
    assert!(matches!(
        &view.endpoints[1].connection.target,
        ConnectTarget::Ssh { target, session } if session == "work" && target.command.is_default()
    ));
    assert!(Arc::ptr_eq(
      &other_inbox,
      &view.endpoints[2].connection.inbox
    ));
  });
}
