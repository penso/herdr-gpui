//! Painting a workspace dialog: its header, the body each action asks for,
//! and the footer whose submit button arms only once there is something to send.

use crate::{
    HerdrWindow,
    i18n::fl,
    menu::{WorkspaceAction, WorkspaceMenuAction, danger, workspace_close::checking},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// The checkout the daemon would create for the branch currently drafted.
    /// Only a preview: the daemon derives the path it actually uses.
    pub(in crate::menu) fn checkout_preview(&self) -> String {
        let repo = self
            .menu
            .target
            .as_ref()
            .and_then(|target| target.worktree.as_ref())
            .map(|worktree| worktree.label.as_str());
        let root = self
            .live
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.worktree_directory.as_str())
            .filter(|root| !root.is_empty());
        let branch = self
            .menu
            .input
            .as_ref()
            .map_or("", |input| input.text.trim());
        match (repo, root) {
            _ if branch.is_empty() => fl!("workspace-dialog-daemon-names"),
            (Some(repo), Some(root)) => crate::worktree::checkout_preview(root, repo, branch),
            _ => fl!("workspace-dialog-daemon-chooses"),
        }
    }

    pub(in crate::menu) fn render_workspace_dialog(
        &self,
        action: WorkspaceAction,
        cx: &mut Context<Self>,
    ) -> Div {
        let Some(target) = &self.menu.target else {
            return div();
        };
        let theme = &self.theme;
        let font = &self.config.ui;
        let danger = danger(theme);
        let deletion = self.menu.deletion.as_ref();
        let force = deletion.is_some_and(|deletion| deletion.force);
        let removing = action == WorkspaceAction::DeleteWorktree
            && self.removal.as_ref().is_some_and(|removal| {
                removal.pending_for(self.menu.endpoint_target, &target.boot_id, &target.id)
            });
        let dispatching = self.dispatch_job.is_some()
            && matches!(
                action,
                WorkspaceAction::NewWorktree | WorkspaceAction::NewWorkspace
            );
        let creating = self.menu.creation.is_some() || dispatching;
        // Another host chosen in the picker, where the new checkout goes.
        let destination = self
            .menu
            .dispatch
            .as_ref()
            .map_or(crate::dispatch::Destination::Here, |picker| {
                picker.destination()
            });
        let dispatched = match destination {
            crate::dispatch::Destination::Host(host) => Some(host),
            _ => None,
        };
        // A new machine the provider makes first, where the checkout goes.
        let sandbox = match destination {
            crate::dispatch::Destination::Sandbox(sandbox) => {
                Some(crate::dispatch::sandbox_words(sandbox))
            }
            _ => None,
        };
        // Until the daemon names the checkout there is nothing to confirm, and a
        // request already in flight leaves nothing to press either.
        let armed = (action != WorkspaceAction::DeleteWorktree
            || deletion.is_some_and(|deletion| deletion.ready()))
            && (action != WorkspaceAction::OpenWorktree
                || self.menu.worktree_open.as_ref().is_some_and(|picker| {
                    picker.pending.is_none()
                        && !picker.filtered.is_empty()
                        && !picker.search.read(cx).is_composing()
                }))
            && (action != WorkspaceAction::Close
                || self.menu.close_check.as_ref().is_some_and(|check| {
                    check.allows(
                        self.menu
                            .input
                            .as_ref()
                            .map_or("", |input| input.text.as_str()),
                        self.config.confirm_close_workspace,
                    )
                }))
            && !creating
            && !removing;
        let destructive = matches!(
            action,
            WorkspaceAction::Close | WorkspaceAction::DeleteWorktree
        );
        let (title, submit): (String, SharedString) = match action {
            WorkspaceAction::Rename => (
                fl!("workspace-dialog-rename-title"),
                fl!("workspace-dialog-rename").into(),
            ),
            WorkspaceAction::Close => (
                target.close_label().text(),
                target.close_label().text().into(),
            ),
            WorkspaceAction::NewWorktree => (
                fl!("workspace-dialog-new-worktree-title"),
                create_label(destination),
            ),
            WorkspaceAction::OpenWorktree => (
                fl!("workspace-dialog-open-worktree-title"),
                fl!("workspace-dialog-open").into(),
            ),
            WorkspaceAction::NewTab => (
                fl!("workspace-dialog-new-tab-title"),
                fl!("workspace-dialog-create").into(),
            ),
            WorkspaceAction::NewWorkspace => (
                fl!("workspace-dialog-new-workspace-title"),
                create_label(destination),
            ),
            WorkspaceAction::Note => (
                fl!("workspace-dialog-note-title"),
                fl!("workspace-dialog-save").into(),
            ),
            WorkspaceAction::DeleteWorktree if force => (
                fl!("workspace-dialog-force-delete-title"),
                fl!("workspace-dialog-force-remove").into(),
            ),
            WorkspaceAction::DeleteWorktree => (
                fl!("workspace-dialog-delete-title"),
                fl!("workspace-dialog-remove").into(),
            ),
        };
        let mut body = div().flex().flex_col().gap(px(10.)).px(px(16.)).py(px(12.));
        body = match action {
            WorkspaceAction::OpenWorktree => body,
            WorkspaceAction::Rename => body.child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("workspace-dialog-rename-note")),
            ),
            WorkspaceAction::Note => body.child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("workspace-dialog-note-note")),
            ),
            WorkspaceAction::NewTab => body.child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("workspace-dialog-new-tab-note")),
            ),
            WorkspaceAction::NewWorkspace => body
                .child(
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("workspace-dialog-new-workspace-note")),
                )
                .children(self.menu.dispatch.as_ref().map(|picker| {
                    // The picker's own captions, laid out like the form's.
                    let caption = |caption: String, field: AnyElement| {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .child(
                                div()
                                    .flex_none()
                                    .text_color(rgb(theme.muted))
                                    .child(caption),
                            )
                            .child(div().flex_1().min_w_0().child(field))
                    };
                    self.render_host_picker(picker, caption, cx)
                }))
                .children(dispatched.map(|host| {
                    let host_name = host.label.as_str();
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(if clones(host) {
                            fl!("workspace-dialog-opens-on-clone", host = host_name)
                        } else {
                            fl!("workspace-dialog-opens-on", host = host_name)
                        })
                }))
                .children(sandbox.clone().map(|machine| {
                    div()
                        .text_color(rgb(theme.subtext()))
                        .child(fl!("workspace-dialog-opens-in", machine = machine))
                })),
            WorkspaceAction::Close => {
                body.child(div().text_color(rgb(theme.subtext())).child(fl!(
                    "workspace-dialog-close-note",
                    count = target.close_members.len()
                )))
            }
            WorkspaceAction::NewWorktree => {
                // Captions share a column so both fields start at the same edge.
                let row = |caption: SharedString, field: AnyElement| {
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex_none()
                                .w(px(font.size * 4.5))
                                .text_color(rgb(theme.muted))
                                .child(caption),
                        )
                        .child(div().flex_1().min_w_0().child(field))
                };
                if let Some(source) = &self.menu.worktree {
                    // Set each frame, like the caption beside it, so the
                    // accessible name follows a language change; unchanged
                    // is a no-op.
                    source.name.update(cx, |input, cx| {
                        input.set_label(&fl!("worktree-source-name"), cx);
                    });
                }
                body.children(self.menu.worktree.as_ref().map(|source| {
                    row(
                        fl!("workspace-dialog-name").into(),
                        div()
                            .debug_selector(|| "worktree-name".into())
                            .child(source.name.clone())
                            .into_any_element(),
                    )
                }))
                .child(row(
                    fl!("workspace-dialog-branch").into(),
                    self.render_dialog_input(cx).into_any_element(),
                ))
                .children(self.menu.dispatch.as_ref().map(|picker| {
                    // The picker's own captions, in English until dispatch's
                    // text is messages.
                    self.render_host_picker(picker, |caption, field| row(caption.into(), field), cx)
                }))
                // Agents start through this host's CLI, so only a checkout
                // made here can land in one.
                .children(
                    self.menu
                        .landing
                        .as_ref()
                        .filter(|_| destination == crate::dispatch::Destination::Here)
                        .map(|picker| {
                            row(
                                fl!("workspace-dialog-lands-in").into(),
                                self.render_landing(picker, cx),
                            )
                        }),
                )
                .child(match (dispatched, &sandbox) {
                    (Some(host), _) if clones(host) => fl!(
                        "workspace-dialog-creates-on-clone",
                        base = target.base_label(),
                        host = host.label.as_str(),
                    ),
                    (Some(host), _) => fl!(
                        "workspace-dialog-creates-on",
                        base = target.base_label(),
                        host = host.label.as_str(),
                    ),
                    (None, Some(machine)) => fl!(
                        "workspace-dialog-creates-in",
                        machine = machine.as_str(),
                        base = target.base_label(),
                    ),
                    (None, None) => {
                        fl!("workspace-dialog-creates-here", base = target.base_label())
                    }
                })
                .child(
                    div()
                        .debug_selector(|| "dialog-checkout".into())
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .px(px(10.))
                        .py(px(6.))
                        .child(match (dispatched, &sandbox) {
                            (Some(host), _) => {
                                fl!("workspace-dialog-host-chooses", host = host.label.as_str())
                            }
                            (None, Some(_)) => fl!("workspace-dialog-its-daemon-chooses"),
                            (None, None) => self.checkout_preview(),
                        }),
                )
            }
            WorkspaceAction::DeleteWorktree => body
                .children(deletion.and_then(|deletion| deletion.merged).map(|number| {
                    div()
                        .debug_selector(|| "dialog-merged-pr".into())
                        .child(fl!("workspace-dialog-merged", number = number))
                }))
                .child(if force {
                    fl!("workspace-dialog-force-removes")
                } else {
                    fl!("workspace-dialog-removes")
                })
                .child(
                    div()
                        .debug_selector(|| "dialog-path".into())
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .px(px(10.))
                        .py(px(6.))
                        .child(
                            deletion
                                .and_then(|deletion| deletion.path.clone())
                                .unwrap_or_else(|| fl!("workspace-dialog-path-pending")),
                        ),
                )
                .child(div().text_color(rgb(theme.subtext())).child(if force {
                    fl!("workspace-dialog-force-note")
                } else {
                    fl!("workspace-dialog-delete-note")
                }))
                .children(deletion.and_then(|deletion| self.archive_note(target, deletion, cx))),
        };
        if action == WorkspaceAction::Close {
            body = body.child(
                div()
                    .debug_selector(|| "close-git-status".into())
                    .text_color(danger)
                    .child(
                        self.menu
                            .close_check
                            .as_ref()
                            .map_or_else(checking, |check| {
                                check.status(self.config.confirm_close_workspace)
                            }),
                    ),
            );
        }
        if self.menu.input.is_some() && action != WorkspaceAction::NewWorktree {
            body = body.child(self.render_dialog_input(cx));
        }
        if let Some(error) = &self.menu.error {
            body = body.child(
                div()
                    .debug_selector(|| "dialog-error".into())
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.active))
                    .px(px(10.))
                    .py(px(6.))
                    .text_color(danger)
                    .child(error.clone()),
            );
        }
        if creating || removing {
            // Dismissing only closes the panel; the daemon keeps the queued work.
            let waiting = match self.dispatch_job.as_ref().filter(|_| dispatching) {
                Some(job) => fl!("workspace-dialog-job-waiting", status = job.status()),
                None => fl!("workspace-dialog-daemon-waiting"),
            };
            body = body.child(
                div()
                    .debug_selector(|| "dialog-waiting".into())
                    .text_color(rgb(theme.subtext()))
                    .child(waiting),
            );
        }
        let button = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(12.))
                .py(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .border_1()
                .cursor_pointer()
        };
        // A picker owns the panel's height, so its list scrolls inside the
        // dialog instead of growing it past the window.
        let listing = action == WorkspaceAction::OpenWorktree || self.worktree_listing();
        div()
            .flex()
            .flex_col()
            .when(
                listing || action == WorkspaceAction::NewWorktree,
                |dialog| dialog.size_full().min_h_0(),
            )
            .child(
                div()
                    .debug_selector(|| "dialog-header".into())
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .when_some(
                        WorkspaceMenuAction::Dialog(action).icon(),
                        |header, icon| {
                            header.child(svg().path(icon).size(px(16.)).flex_none().text_color(
                                if destructive {
                                    danger
                                } else {
                                    rgb(theme.muted)
                                },
                            ))
                        },
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .debug_selector(|| "dialog-title".into())
                                    .when(action == WorkspaceAction::OpenWorktree, |title| {
                                        title.truncate()
                                    })
                                    .text_size(px(font.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_color(rgb(theme.muted))
                                    .child(target.label.clone()),
                            ),
                    )
                    .when(action == WorkspaceAction::OpenWorktree, |header| {
                        header.child(
                            div()
                                .id("open-worktree-escape")
                                .debug_selector(|| "open-worktree-escape".into())
                                .flex_none()
                                .px_2()
                                .py_1()
                                .rounded(px(crate::config::corners::CONTROL))
                                .cursor_pointer()
                                .text_color(rgb(theme.muted))
                                .hover(|button| {
                                    button
                                        .bg(rgb(theme.active))
                                        .text_color(rgb(theme.foreground))
                                })
                                .child("ESC")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.dismiss_menu(window, cx);
                                })),
                        )
                    }),
            )
            .when(action == WorkspaceAction::NewWorktree, |dialog| {
                dialog.child(self.render_worktree_tabs(cx))
            })
            .child(if action == WorkspaceAction::OpenWorktree {
                self.render_existing_worktrees(cx)
                    .when(creating || self.menu.error.is_some(), |picker| {
                        picker.child(
                            div()
                                .id("open-worktree-status")
                                .debug_selector(|| "open-worktree-status".into())
                                .flex_none()
                                .h(px(font.line_height() * 3. + 20.))
                                .overflow_y_scroll()
                                .child(body),
                        )
                    })
                    .into_any_element()
            } else if listing {
                self.render_worktree_items(cx).into_any_element()
            } else if action == WorkspaceAction::NewWorktree {
                // The branch form shares the listings' settled height, so it
                // fills the space above the footer and scrolls within it.
                div()
                    .id("new-worktree-form")
                    .debug_selector(|| "new-worktree-form".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(body)
                    .into_any_element()
            } else {
                body.into_any_element()
            })
            .child(
                div()
                    .debug_selector(|| "dialog-footer".into())
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .child(
                        button("dialog-cancel")
                            .role(Role::Button)
                            .aria_label(fl!("workspace-dialog-cancel"))
                            .border_color(rgb(theme.active))
                            .hover(|button| button.bg(rgb(theme.active)))
                            .child(fl!("workspace-dialog-cancel"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                cx.stop_propagation();
                                this.dismiss_menu(window, cx);
                            })),
                    )
                    // GitHub rows create their own checkouts without a submit button.
                    .when(
                        !listing || action == WorkspaceAction::OpenWorktree,
                        |footer| {
                            let spoken = if creating {
                                SharedString::from(fl!("workspace-dialog-waiting-spoken"))
                            } else {
                                submit.clone()
                            };
                            let submit_button = crate::accessibility::disabled(
                                button("dialog-submit")
                                    .role(Role::DefaultButton)
                                    .aria_label(spoken),
                                !armed,
                            );
                            footer.child(
                                submit_button
                                    // The primary action carries the fill; a
                                    // destructive one also carries the warning hue.
                                    .border_color(if !armed {
                                        rgb(theme.active)
                                    } else if destructive {
                                        danger
                                    } else {
                                        rgb(theme.foreground)
                                    })
                                    .when(armed, |button| button.bg(rgb(theme.active)))
                                    .text_color(if !armed {
                                        rgb(theme.muted)
                                    } else if destructive {
                                        danger
                                    } else {
                                        rgb(theme.foreground)
                                    })
                                    .child(if creating {
                                        fl!("workspace-dialog-waiting").into()
                                    } else {
                                        submit
                                    })
                                    // Faded and inert until there is something
                                    // to submit, so it never reads as pressable.
                                    .when(!armed, |button| {
                                        button.opacity(0.4).cursor(CursorStyle::OperationNotAllowed)
                                    })
                                    .when(armed, |button| {
                                        button
                                            .hover(|button| button.bg(rgb(theme.active)))
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.submit_workspace_dialog(window, cx);
                                            }))
                                    }),
                            )
                        },
                    ),
            )
    }
}

/// The submit button of a dialog that may create on another host.
fn create_label(destination: crate::dispatch::Destination<'_>) -> SharedString {
    match destination {
        crate::dispatch::Destination::Here => fl!("workspace-dialog-create").into(),
        crate::dispatch::Destination::Host(host) => {
            fl!("workspace-dialog-create-on", host = host.label.as_str()).into()
        }
        crate::dispatch::Destination::Sandbox(sandbox) => fl!(
            "workspace-dialog-create-in",
            machine = crate::dispatch::sandbox_words(sandbox)
        )
        .into(),
    }
}

/// Whether choosing `host` clones the repository there first.
fn clones(host: &crate::dispatch::Candidate) -> bool {
    host.repository == crate::dispatch::Repository::Missing
}
