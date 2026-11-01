//! Drawing the account menu's Azure DevOps page: a short help line, a card
//! for each organization, and the form that checks and saves a token.

use super::{Lookup, OrganizationStatus, status_line};
use crate::i18n::fl;
use crate::{HerdrWindow, menu::Page, search_input::SearchInput};
use gpui::{prelude::*, *};

/// The Azure DevOps mark, drawn in one color as the other providers' are.
pub(crate) fn azure_mark(color: u32) -> Svg {
    svg()
        .path("icons/azure-devops.svg")
        .flex_none()
        .text_color(rgb(color))
}

impl HerdrWindow {
    /// An organization's card: the account's initial and name, the
    /// organization with where its token comes from, and Forget for a token
    /// saved here.
    fn render_azure_organization(
        &self,
        index: usize,
        status: &OrganizationStatus,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let account = match &status.lookup {
            Lookup::Found {
                account: Some(Ok(account)),
                ..
            } => Some(account),
            _ => None,
        };
        let initial = account
            .and_then(|account| account.name.chars().next())
            .map(|initial| initial.to_uppercase().to_string());
        let title = account.map_or_else(
            || status.organization.clone(),
            |account| account.name.clone(),
        );
        let ready = !self.menu.azure.working;
        let organization = status.organization.clone();
        let forget = status.saved.then(|| {
            self.gitlab_button(
                format!("azure-forget-{index}"),
                fl!("azure-devops-menu-forget"),
                false,
                ready,
            )
            .when(ready, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.forget_azure(organization.clone(), cx);
                }))
            })
        });
        div()
            .debug_selector(move || format!("azure-organization-{index}"))
            .flex()
            .items_center()
            .gap(px(12.))
            .p(px(12.))
            .rounded(px(crate::config::corners::PANEL))
            .border_1()
            .border_color(rgb(theme.active))
            .bg(rgb(theme.background))
            .child(
                div()
                    .debug_selector(move || format!("azure-avatar-{index}"))
                    .flex_none()
                    .size(px(36.))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(theme.active))
                    .child(match initial {
                        Some(initial) => div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(initial)
                            .into_any_element(),
                        None => azure_mark(theme.muted).size(px(20.)).into_any_element(),
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .when(account.is_some(), |column| {
                        column.child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(theme.muted))
                                .child(status.organization.clone()),
                        )
                    })
                    .child(
                        div()
                            .debug_selector(move || format!("azure-status-{index}"))
                            .text_color(rgb(theme.subtext()))
                            .child(status_line(status)),
                    ),
            )
            .children(forget)
    }

    pub(in crate::menu) fn render_azure_page(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let azure = &self.menu.azure;
        let ready = !azure.working;
        let field = |label: String, selector: &'static str, input: &Option<Entity<SearchInput>>| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(label)
                .children(input.as_ref().map(|input| {
                    div()
                        .debug_selector(move || selector.into())
                        .child(input.clone())
                }))
        };
        let accent = crate::menu::accent(theme);
        let body = div()
            .id("azure-body")
            .debug_selector(|| "azure-body".into())
            .min_h_0()
            .overflow_y_scroll()
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("azure-devops-menu-help")),
            )
            .children(azure.problem.clone().map(|problem| {
                div()
                    .debug_selector(|| "azure-problem".into())
                    .text_color(rgb(theme.ink(theme.palette[1])))
                    .child(problem)
            }))
            .children(
                azure
                    .organizations
                    .iter()
                    .enumerate()
                    .map(|(index, status)| self.render_azure_organization(index, status, cx)),
            )
            .child(field(
                fl!("azure-devops-menu-organization"),
                "azure-organization-input",
                &azure.organization,
            ))
            .child(field(
                fl!("azure-devops-menu-token"),
                "azure-token-input",
                &azure.token,
            ))
            .child(
                div()
                    .id("azure-create-token")
                    .debug_selector(|| "azure-create-token".into())
                    .role(Role::Link)
                    .aria_label(fl!("azure-devops-menu-create-token"))
                    .cursor_pointer()
                    .text_color(accent)
                    .hover(|style| style.underline())
                    .child(fl!("azure-devops-menu-create-token"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.open_azure_token_page(cx);
                    })),
            )
            .children(azure.message.clone().map(|message| {
                div()
                    .debug_selector(|| "azure-message".into())
                    .p(px(10.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .bg(rgb(theme.background))
                    .child(message)
            }));
        let footer = div()
            .flex_none()
            .p(px(12.))
            .border_t_1()
            .border_color(rgb(theme.active))
            .flex()
            .justify_end()
            .gap(px(8.))
            .child(
                self.gitlab_button(
                    "azure-save",
                    fl!("azure-devops-menu-check-and-save"),
                    true,
                    ready,
                )
                .when(ready, |button| {
                    button.on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.save_azure(cx);
                    }))
                }),
            );
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_h_0()
            .child(self.render_account_tabs(Page::AzureDevOps, cx))
            .child(body)
            .child(footer)
    }
}
