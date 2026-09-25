//! The sidebar host header's context menu. Only saved SSH devices have one:
//! Local is always present, and an explicit socket is not in the catalog.
use super::setup;
use crate::{HerdrWindow, github::Account, menu::Page, search_input::SearchInput};
use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Rename,
    Remove,
}

const ACTIONS: [(Action, &str); 2] = [
    (Action::Rename, "Rename…"),
    (Action::Remove, "Remove device…"),
];

pub(crate) struct HostMenu {
    /// The endpoint, as the window and its account slots key it.
    id: String,
    /// The catalog profile the `herdr machine` commands name.
    profile: String,
    label: String,
    target: String,
    session: String,
    selected: Option<usize>,
    /// Also delete the device's own GitHub sign-in. Offered only when it has
    /// one, since its account panel disappears with the device.
    forget_github: bool,
    /// The new name while renaming.
    input: Option<Entity<SearchInput>>,
    renaming: bool,
    error: Option<String>,
}

impl HerdrWindow {
    pub(crate) fn open_host_menu(
        &mut self,
        id: &str,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The CLI edits the default catalog, the one device setup also uses.
        // A device already being removed has nothing left to offer.
        if self.device_setup_unavailable().is_some() || self.menu.removing_devices.contains(id) {
            return;
        }
        let Some(endpoint) = self.endpoints.iter().find(|endpoint| endpoint.id == id) else {
            return;
        };
        // The saved entry, not the live target: removal claims and names the
        // profile, whichever session the list has attached the device to.
        let Some((target, session)) = endpoint.saved_ssh() else {
            return;
        };
        let Some(profile) = crate::endpoint::saved_profile_id(&endpoint.id) else {
            return;
        };
        let menu = HostMenu {
            id: endpoint.id.clone(),
            profile: profile.to_owned(),
            label: endpoint.label.clone(),
            target: target.to_owned(),
            session: session.to_owned(),
            selected: None,
            forget_github: true,
            input: None,
            renaming: false,
            error: None,
        };
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Host);
        self.menu.host = Some(menu);
    }

    /// Whether the device has a GitHub sign-in of its own saved on this computer.
    fn host_has_github(&self, id: &str) -> bool {
        self.menu
            .github_hosts
            .get(id)
            .is_some_and(|auth| auth.can_sign_out())
    }

    fn activate_host_menu(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            Action::Rename => {
                let Some(host) = &self.menu.host else {
                    return;
                };
                let (label, target) = (host.label.clone(), host.target.clone());
                let input = cx.new(SearchInput::new);
                input.update(cx, |input, cx| {
                    input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
                    // An empty name falls back to the target, as when adding.
                    input.set_placeholder(&target, cx);
                    input.set_text_selected(&label, cx);
                    window.focus(&input.focus, cx);
                });
                if let Some(host) = &mut self.menu.host {
                    host.input = Some(input);
                    host.error = None;
                }
                self.menu.page = Some(Page::RenameDevice);
            }
            Action::Remove => self.menu.page = Some(Page::RemoveDevice),
        }
        cx.notify();
    }

    /// Rename in the background and close once the catalog has the new name;
    /// the sidebar and picker pick it up from there.
    fn submit_rename_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = &mut self.menu.host else {
            return;
        };
        let Some(input) = &host.input else {
            return;
        };
        if host.renaming || input.read(cx).is_composing() {
            return;
        }
        let label = match setup::device_label(input.read(cx).text(), &host.target) {
            Ok(label) => label,
            Err(error) => {
                host.error = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let (id, profile) = (host.id.clone(), host.profile.clone());
        host.renaming = true;
        host.error = None;
        let background = cx
            .background_executor()
            .spawn(async move { setup::rename(&profile, &label) });
        cx.spawn_in(window, async move |this, cx| {
            let result = background.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // The menu may have closed or moved to another device meanwhile.
                let Some(host) = this
                    .menu
                    .host
                    .as_mut()
                    .filter(|host| host.id == id && host.renaming)
                else {
                    return;
                };
                host.renaming = false;
                match result {
                    Ok(()) => this.dismiss_menu(window, cx),
                    Err(error) => {
                        host.error = Some(error.to_string());
                        if let Some(input) = &host.input {
                            window.focus(&input.read(cx).focus.clone(), cx);
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Close the confirmation at once and remove in the background. Until the
    /// catalog drops the device, its sidebar header pulses the way a worktree
    /// row does while it is removed; a failure is reported in the status bar.
    fn confirm_remove_device(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.menu.host.take() else {
            return;
        };
        let forget = host.forget_github && self.host_has_github(&host.id);
        let store = self.menu.github_hosts.get(&host.id).map_or_else(
            || crate::github::Store::select(&self.config),
            |auth| auth.store(),
        );
        let HostMenu {
            id,
            profile,
            label,
            target,
            session,
            ..
        } = host;
        // The outer result is the removal itself; the inner one, the GitHub
        // credential that only matters once the device is gone.
        let background = cx.background_executor().spawn(async move {
            let _claim = setup::claim_saved(&target, &session)?;
            setup::remove(&profile)?;
            Ok::<_, crate::Error>(match Account::host(&profile).filter(|_| forget) {
                Some(account) => crate::github::forget(store, &account),
                None => Ok(()),
            })
        });
        self.menu.removing_devices.insert(id.clone());
        self.dismiss_menu(window, cx);
        cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| {
                this.device_removed(&id, &label, forget, result, cx)
            });
        })
        .detach();
    }

    fn device_removed(
        &mut self,
        id: &str,
        label: &str,
        forget: bool,
        result: crate::Result<crate::Result<()>>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Err(error) => {
                self.menu.removing_devices.remove(id);
                self.local_error = Some(format!("Remove {label}: {error}"));
            }
            // The catalog watcher drops the device, and a filter or selection
            // on it falls back to Local, as for any removal. The header keeps
            // pulsing until then; `prune_device_removals` clears the mark.
            Ok(forgotten) => {
                if forget {
                    self.menu.github_hosts.remove(id);
                }
                if let Err(error) = forgotten {
                    self.local_error = Some(format!(
                        "Removed {label}, but could not delete its GitHub sign-in: {error}"
                    ));
                }
            }
        }
        cx.notify();
    }

    /// Forget removal marks for devices the catalog no longer lists.
    pub(crate) fn prune_device_removals(&mut self) {
        let endpoints = &self.endpoints;
        self.menu
            .removing_devices
            .retain(|id| endpoints.iter().any(|endpoint| &endpoint.id == id));
    }

    pub(in crate::menu) fn host_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &mut self.menu.host else {
            return;
        };
        let key = event.keystroke.key.as_str();
        // The name field owns typing and composition; only its submit and
        // cancel keys belong to the menu.
        if self.menu.page == Some(Page::RenameDevice)
            && !host.renaming
            && (host
                .input
                .as_ref()
                .is_some_and(|input| input.read(cx).is_composing())
                || !matches!(key, "escape" | "enter"))
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        match key {
            "escape" => self.dismiss_menu(window, cx),
            "enter" if self.menu.page == Some(Page::RenameDevice) => {
                self.submit_rename_device(window, cx)
            }
            "enter" if self.menu.page == Some(Page::RemoveDevice) => {
                self.confirm_remove_device(window, cx)
            }
            "space" if self.menu.page == Some(Page::RemoveDevice) => {
                host.forget_github = !host.forget_github;
                cx.notify();
            }
            key @ ("up" | "down") if self.menu.page == Some(Page::Host) => {
                host.selected = Some(match (host.selected, key) {
                    (None, "up") => ACTIONS.len() - 1,
                    (None, _) => 0,
                    (Some(i), "up") => (i + ACTIONS.len() - 1) % ACTIONS.len(),
                    (Some(i), _) => (i + 1) % ACTIONS.len(),
                });
                cx.notify();
            }
            "enter" => {
                if let Some(index) = host.selected {
                    self.activate_host_menu(ACTIONS[index].0, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(in crate::menu) fn render_host_menu(&self, cx: &mut Context<Self>) -> Div {
        let Some(host) = &self.menu.host else {
            return div();
        };
        let theme = &self.theme;
        let mut body = div().flex().flex_col();
        if self.menu.page == Some(Page::Host) {
            // Name the device first, so the destructive row below cannot be
            // mistaken for acting on another host.
            body = body.child(
                div()
                    .debug_selector(|| "host-menu-header".into())
                    .px(px(8.))
                    .pt(px(4.))
                    .pb(px(8.))
                    .mb(px(4.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(host.label.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(self.config.ui.size * 0.85))
                            .text_color(rgb(theme.muted))
                            .truncate()
                            .child(format!("{} · {}", host.target, host.session)),
                    ),
            );
            for (index, (action, label)) in ACTIONS.into_iter().enumerate() {
                body = body.child(
                    div()
                        .id(("host-menu-action", index))
                        .debug_selector(move || format!("host-menu-{index}"))
                        .min_h(px(self.config.ui.line_height() + 12.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .when(action == Action::Remove, |row| {
                            row.text_color(crate::menu::danger(theme))
                        })
                        .when(host.selected == Some(index), |row| {
                            row.bg(rgb(theme.active))
                        })
                        .hover(|row| row.bg(rgb(theme.active)))
                        .child(label)
                        .on_hover(cx.listener(move |this, hovered, _, cx| {
                            if *hovered && let Some(host) = &mut this.menu.host {
                                host.selected = Some(index);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.activate_host_menu(action, window, cx)
                        })),
                );
            }
            return body;
        }
        if self.menu.page == Some(Page::RenameDevice) {
            return self.render_rename_device(host, cx);
        }
        let github = self.host_has_github(&host.id);
        body = body
            .debug_selector(|| "remove-device".into())
            .p(px(8.))
            .gap(px(12.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Remove device"),
            )
            .child(format!(
                "Remove {} ({} · {}) from this computer? Herdr keeps running on that host.",
                host.label, host.target, host.session
            ))
            .when(github, |body| {
                body.child(
                    div()
                        .id("remove-device-github")
                        .debug_selector(|| "remove-device-github".into())
                        .flex()
                        .gap(px(8.))
                        .cursor_pointer()
                        .child(if host.forget_github { "☑" } else { "☐" })
                        .child("Also delete its GitHub sign-in from this computer")
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(host) = &mut this.menu.host {
                                host.forget_github = !host.forget_github;
                                cx.notify();
                            }
                        })),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("remove-device-cancel")
                            .p(px(6.))
                            .cursor_pointer()
                            .child("Cancel")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .id("remove-device-submit")
                            .debug_selector(|| "remove-device-submit".into())
                            .p(px(6.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(theme.active))
                            .text_color(crate::menu::danger(theme))
                            .cursor_pointer()
                            .child("Remove")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm_remove_device(window, cx)
                            })),
                    ),
            );
        body
    }

    /// A centered modal like Add Device: header with the title and Close,
    /// the name field in the body, and the action in the footer.
    fn render_rename_device(&self, host: &HostMenu, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let header = div()
            .debug_selector(|| "rename-device-header".into())
            .flex_none()
            .p(px(16.))
            .border_b_1()
            .border_color(rgb(theme.active))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(self.config.ui.size * 1.35))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Rename device"),
            )
            .child(
                div()
                    .id("rename-device-close")
                    .debug_selector(|| "rename-device-close".into())
                    .px_2()
                    .py_1()
                    .cursor_pointer()
                    .rounded(px(crate::config::corners::CONTROL))
                    .hover(|s| s.bg(rgb(theme.active)))
                    .child("Close")
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx))),
            );
        let body = div()
            .debug_selector(|| "rename-device".into())
            .min_h_0()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(format!("{} · {}", host.target, host.session)),
            )
            .child(div().flex().flex_col().gap(px(6.)).child("Name").when_some(
                host.input.clone(),
                |field, input| {
                    if host.renaming {
                        field.child(div().child(input.read(cx).text().to_owned()))
                    } else {
                        field.child(input)
                    }
                },
            ))
            .when_some(host.error.clone(), |body, error| {
                body.child(div().text_color(crate::menu::danger(theme)).child(error))
            });
        let ready = !host.renaming;
        let footer = div()
            .debug_selector(|| "rename-device-footer".into())
            .flex_none()
            .p(px(16.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .justify_end()
            .child(
                div()
                    .id("rename-device-submit")
                    .debug_selector(|| "rename-device-submit".into())
                    .p(px(8.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.active))
                    .when(ready, |button| {
                        button.cursor_pointer().hover(|s| {
                            s.bg(rgb(theme.active).blend(rgba((theme.foreground << 8) | 0x20)))
                        })
                    })
                    .when(!ready, |button| button.text_color(rgb(theme.muted)))
                    .child(if ready { "Rename" } else { "Renaming…" })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.submit_rename_device(window, cx)),
                    ),
            );
        div()
            .flex()
            .flex_col()
            .min_h_0()
            .child(header)
            .child(body)
            .child(footer)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Action, HostMenu};
    use crate::{menu::Page, sidebar::layout_tests::fixture_window};
    use gpui::{Modifiers, MouseButton, TestAppContext, point, px, size};

    // Endpoint IDs carry the `ssh:` prefix; the profile ID is the rest.
    const HOST: &str = "ssh:0123456789abcdef0123456789abcdef";
    const HOST_HEADER: &str = "host-ssh:0123456789abcdef0123456789abcdef";
    const LOCAL_HEADER: &str = "host-local";

    fn add_host(view: &mut crate::HerdrWindow) {
        view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
        // Fold Local's many fixture workspaces so the new host's header is on screen.
        view.endpoints[0].collapsed = true;
        view.endpoints.push(crate::endpoint::Endpoint::new(
            HOST.into(),
            "m5max-ms".into(),
            herdr_client::ConnectTarget::Ssh {
                target: "penso@box".into(),
                session: "default".into(),
            },
            true,
        ));
    }

    fn host(view: &crate::HerdrWindow) -> &HostMenu {
        view.menu.host.as_ref().unwrap()
    }

    #[gpui::test]
    fn right_clicking_a_saved_host_offers_removal_but_local_has_no_menu(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            crate::bind_keys(cx);
            let mut view = fixture_window(window, cx);
            add_host(&mut view);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let local = cx.debug_bounds(LOCAL_HEADER).unwrap();
        cx.simulate_mouse_down(local.center(), MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(local.center(), MouseButton::Right, Modifiers::default());
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));

        let remote = cx.debug_bounds(HOST_HEADER).unwrap();
        cx.simulate_mouse_down(remote.center(), MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(remote.center(), MouseButton::Right, Modifiers::default());
        // Saved SSH devices are unavailable on Windows, so there is nothing to remove.
        if cfg!(windows) {
            assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
            return;
        }
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, Some(Page::Host));
            assert_eq!(host(view).target, "penso@box");
            // `herdr machine remove` takes the bare catalog ID.
            assert_eq!(host(view).profile, "0123456789abcdef0123456789abcdef");
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let header = cx.debug_bounds("host-menu-header").unwrap();
        assert!(header.bottom() <= cx.debug_bounds("host-menu-0").unwrap().top());
        // Rename comes first; Remove is last, in the danger color.
        cx.simulate_keystrokes("down down enter");
        assert!(view.read_with(cx, |view, _| view.menu.page == Some(Page::RemoveDevice)));
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        assert!(cx.debug_bounds("remove-device").is_some());
        // Without a GitHub sign-in of its own, there is nothing else to delete.
        assert!(cx.debug_bounds("remove-device-github").is_none());
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    }

    /// The menu speaks for the saved device: after the sessions list attached it
    /// to another session, it still names, and removal still claims, the session
    /// the device was saved with.
    #[gpui::test]
    fn the_menu_names_the_saved_session_after_a_session_pick(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        view.update_in(cx, |view, window, cx| {
            view.endpoints[0].connection.target = herdr_client::ConnectTarget::Local;
            view.reconcile_catalog(
                vec![herdr_client::SavedHost {
                    id: "0123456789abcdef0123456789abcdef".into(),
                    label: "m5max-ms".into(),
                    target: "penso@box".into(),
                    session: "default".into(),
                    enabled: true,
                }],
                cx,
            );
            // What choosing another session from the list does to the target.
            view.endpoints[1].connection.target = herdr_client::ConnectTarget::Ssh {
                target: "penso@box".into(),
                session: "work".into(),
            };
            view.open_host_menu(HOST, point(px(0.), px(0.)), window, cx);
            // Saved SSH devices are unavailable on Windows, so there is no menu.
            if cfg!(windows) {
                assert!(view.menu.host.is_none());
                return;
            }
            assert_eq!(host(view).target, "penso@box");
            assert_eq!(host(view).session, "default");
        });
    }

    #[gpui::test]
    fn removal_offers_the_github_sign_in_and_reports_each_outcome(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                add_host(view);
                view.menu
                    .github_hosts
                    .insert(HOST.into(), crate::github::Auth::connected_fixture());
                view.open_host_menu(HOST, point(px(10.), px(10.)), window, cx);
                if !cfg!(windows) {
                    view.activate_host_menu(Action::Remove, window, cx);
                }
            });
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
        if cfg!(windows) {
            assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
            return;
        }
        assert!(cx.debug_bounds("remove-device-github").is_some());
        cx.simulate_keystrokes("space");
        assert!(view.read_with(cx, |view, _| !host(view).forget_github));
        cx.simulate_keystrokes("space");
    }

    #[gpui::test]
    fn a_removal_pulses_the_host_header_until_the_device_leaves(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                add_host(view);
                view.menu
                    .github_hosts
                    .insert(HOST.into(), crate::github::Auth::connected_fixture());
                // As confirming does: the dialog is gone, the device marked.
                view.menu.removing_devices.insert(HOST.into());
                cx.notify();
            });
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
        let dot = cx.debug_bounds("host-removing").unwrap();
        assert!(
            cx.debug_bounds(HOST_HEADER)
                .unwrap()
                .contains(&dot.center())
        );
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                // No second removal while one is running.
                view.open_host_menu(HOST, point(px(10.), px(10.)), window, cx);
                assert!(view.menu.page.is_none());

                // A failure clears the mark and says why in the status bar.
                view.device_removed(HOST, "m5max-ms", true, Err(crate::Error::DeviceAdding), cx);
                assert!(view.menu.removing_devices.is_empty());
                assert_eq!(
                    view.local_error.as_deref(),
                    Some("Remove m5max-ms: This host is already being added.")
                );
                assert!(view.menu.github_hosts.contains_key(HOST));

                // Success keeps the mark until the catalog drops the device,
                // even when its GitHub credential could not be deleted.
                view.menu.removing_devices.insert(HOST.into());
                view.device_removed(
                    HOST,
                    "m5max-ms",
                    true,
                    Ok(Err(crate::Error::CredentialPolicy)),
                    cx,
                );
                assert!(!view.menu.github_hosts.contains_key(HOST));
                assert!(
                    view.local_error
                        .as_deref()
                        .unwrap()
                        .starts_with("Removed m5max-ms, but could not delete its GitHub sign-in")
                );
                view.prune_device_removals();
                assert!(view.menu.removing_devices.contains(HOST));
                view.endpoints.truncate(1);
                view.prune_device_removals();
                assert!(view.menu.removing_devices.is_empty());
            });
        });
    }

    #[gpui::test]
    fn renaming_edits_the_name_in_place_and_keeps_typing_in_the_field(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            crate::bind_keys(cx);
            let mut view = fixture_window(window, cx);
            add_host(&mut view);
            view
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_host_menu(HOST, point(px(10.), px(10.)), window, cx)
            });
        });
        if cfg!(windows) {
            assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
            return;
        }
        cx.simulate_keystrokes("down enter");
        let input = view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, Some(Page::RenameDevice));
            host(view).input.clone().unwrap()
        });
        cx.update(|window, cx| {
            // The current name is selected, so typing replaces it.
            assert_eq!(input.read(cx).text(), "m5max-ms");
            crate::sidebar::layout_tests::full_draw(window, cx).clear(cx);
        });
        // A modal like Add Device: header above the field, footer below it.
        let header = cx.debug_bounds("rename-device-header").unwrap();
        let body = cx.debug_bounds("rename-device").unwrap();
        let footer = cx.debug_bounds("rename-device-footer").unwrap();
        assert!(header.bottom() <= body.top() && body.bottom() <= footer.top());
        assert!(cx.debug_bounds("rename-device-close").is_some());
        // Centered over the window rather than at the pointer.
        let panel = cx.debug_bounds("menu-panel").unwrap();
        assert!((panel.center().x - px(400.)).abs() <= px(2.));
        // Letters, including the menu's own row keys, go to the field.
        cx.simulate_input("down");
        cx.update(|_, cx| assert_eq!(input.read(cx).text(), "down"));
        assert!(view.read_with(cx, |view, _| view.menu.page == Some(Page::RenameDevice)));
        cx.update(|_, cx| {
            input.update(cx, |input, cx| input.set_text_selected("bad\u{7}name", cx))
        });
        cx.simulate_keystrokes("enter");
        view.read_with(cx, |view, _| {
            assert!(!host(view).renaming);
            assert!(
                host(view)
                    .error
                    .as_deref()
                    .unwrap()
                    .starts_with("Device names are")
            );
        });
        cx.simulate_keystrokes("escape");
        assert!(view.read_with(cx, |view, _| view.menu.page.is_none()));
    }
}
