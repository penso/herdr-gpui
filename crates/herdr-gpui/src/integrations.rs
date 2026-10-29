//! Daemon-host integration management. No native credentials or local file writes.
// i18n: migrated
use crate::{HerdrWindow, connection::IntegrationInbox, i18n::fl};
use gpui::{prelude::*, *};
use herdr_client::Method;
use serde_json::Value;
use std::sync::{Arc, Mutex};

const MAX_ROWS: usize = 64;
const MAX_TEXT: usize = 1024;
const MAX_MESSAGES: usize = 32;

#[derive(Debug, thiserror::Error)]
enum ResponseError {
    #[error("Invalid integration response from the daemon.")]
    Invalid,
    #[error("Integration response exceeds the display limits.")]
    Limit,
    #[error("{code}: {message}")]
    Daemon { code: String, message: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    NotInstalled,
    Current,
    Outdated,
}

#[derive(Debug)]
struct Integration {
    target: String,
    label: String,
    command: String,
    available: bool,
    state: State,
}

impl Integration {
    fn matches(&self, query: &str) -> bool {
        self.label.to_lowercase().contains(query) || self.target.to_lowercase().contains(query)
    }
}

fn text(value: &Value, limit: usize) -> Result<&str, ResponseError> {
    let text = value.as_str().ok_or(ResponseError::Invalid)?;
    if text.len() > limit {
        return Err(ResponseError::Limit);
    }
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(ResponseError::Invalid);
    }
    Ok(text)
}

fn result<'a>(response: &'a Value, kind: &str) -> Result<&'a Value, ResponseError> {
    if let Some(error) = response.get("error").filter(|error| !error.is_null()) {
        return Err(ResponseError::Daemon {
            code: text(&error["code"], 128)?.into(),
            message: text(&error["message"], MAX_TEXT)?.into(),
        });
    }
    let result = &response["result"];
    if result["type"] != kind {
        return Err(ResponseError::Invalid);
    }
    Ok(result)
}

fn parse_list(response: &Value, method: Method) -> Result<Vec<Integration>, ResponseError> {
    let kind = match method {
        Method::IntegrationList => "integration_list",
        Method::IntegrationListV2 => "integration_list_v2",
        _ => return Err(ResponseError::Invalid),
    };
    let rows = result(response, kind)?["integrations"]
        .as_array()
        .ok_or(ResponseError::Invalid)?;
    if rows.len() > MAX_ROWS {
        return Err(ResponseError::Limit);
    }
    let mut integrations = Vec::<Integration>::with_capacity(rows.len());
    for row in rows {
        let target = text(&row["target"], 64)?;
        if !target.bytes().all(|b| {
            b.is_ascii_lowercase()
                || b == b'_'
                || (method == Method::IntegrationListV2 && b == b'-')
        }) || integrations.iter().any(|item| item.target == target)
        {
            return Err(ResponseError::Invalid);
        }
        integrations.push(Integration {
            target: target.into(),
            label: text(&row["label"], 256)?.into(),
            command: text(&row["command"], MAX_TEXT)?.into(),
            available: row["available"].as_bool().ok_or(ResponseError::Invalid)?,
            state: match row["state"].as_str() {
                Some("not_installed") => State::NotInstalled,
                Some("current") => State::Current,
                Some("outdated") => State::Outdated,
                _ => return Err(ResponseError::Invalid),
            },
        });
    }
    integrations.sort_by_cached_key(|row| (row.label.to_lowercase(), row.target.clone()));
    Ok(integrations)
}

fn parse_install(
    response: &Value,
    target: &str,
    method: Method,
) -> Result<Vec<String>, ResponseError> {
    let kind = match method {
        Method::IntegrationInstall => "integration_install",
        Method::IntegrationInstallV2 => "integration_install_v2",
        _ => return Err(ResponseError::Invalid),
    };
    let result = result(response, kind)?;
    if text(&result["target"], 64)? != target {
        return Err(ResponseError::Invalid);
    }
    let messages = result["details"]["messages"]
        .as_array()
        .ok_or(ResponseError::Invalid)?;
    if messages.len() > MAX_MESSAGES {
        return Err(ResponseError::Limit);
    }
    messages
        .iter()
        .map(|value| text(value, MAX_TEXT).map(str::to_owned))
        .collect()
}

#[derive(Clone)]
struct Scope {
    endpoint: String,
    host: String,
    epoch: u64,
    generation: u64,
    boot: String,
    inbox: Arc<Mutex<IntegrationInbox>>,
}

#[derive(Default)]
pub(crate) struct Integrations {
    pub busy: bool,
    pub error: Option<String>,
    pub messages: Vec<String>,
    rows: Vec<Integration>,
    scope: Option<Scope>,
    pending: Option<(String, Operation)>,
    scroll: ScrollHandle,
}

#[derive(Clone)]
enum Operation {
    List(Method),
    Install { target: String, method: Method },
}

impl Integrations {
    /// The listed integrations as `(target, label, state)`; the state is
    /// "not_installed", "current", "outdated", or "unavailable".
    pub(crate) fn listed(&self) -> impl Iterator<Item = (&str, &str, &'static str)> {
        self.rows.iter().map(|row| {
            let state = match row.state {
                _ if !row.available => "unavailable",
                State::NotInstalled => "not_installed",
                State::Current => "current",
                State::Outdated => "outdated",
            };
            (row.target.as_str(), row.label.as_str(), state)
        })
    }

    fn clear_pending(&mut self) {
        if let Some((id, _)) = self.pending.take()
            && let Some(scope) = &self.scope
            && let Ok(mut inbox) = scope.inbox.lock()
            && inbox
                .pending
                .as_ref()
                .is_some_and(|(request, _)| *request == id)
        {
            inbox.pending = None;
        }
        self.busy = false;
    }
}

impl HerdrWindow {
    fn integration_scope_current(&self, scope: &Scope) -> bool {
        self.endpoints
            .get(self.selected_endpoint)
            .is_some_and(|endpoint| {
                endpoint.id == scope.endpoint
                    && endpoint.generation == scope.generation
                    && self.selection_epoch == scope.epoch
                    && Arc::ptr_eq(&endpoint.connection.integrations, &scope.inbox)
                    && endpoint.live.status.is_connected()
                    && endpoint
                        .live
                        .snapshot
                        .as_ref()
                        .is_some_and(|snapshot| snapshot.boot_id == scope.boot)
            })
    }

    pub(crate) fn load_integrations(&mut self, cx: &mut Context<Self>) {
        if self.integrations.busy
            && self
                .integrations
                .scope
                .as_ref()
                .is_some_and(|scope| self.integration_scope_current(scope))
        {
            return;
        }
        self.integrations.clear_pending();
        self.integrations.rows.clear();
        self.integrations.messages.clear();
        self.integrations.error = None;
        self.integrations.scope = None;
        let request = (|| {
            let endpoint = self
                .endpoints
                .get(self.selected_endpoint)
                .ok_or(crate::Error::NotConnected)?;
            if !endpoint.live.status.is_connected() {
                return Err(crate::Error::NotConnected);
            }
            let snapshot = endpoint
                .live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NoSnapshot)?;
            let scope = Scope {
                endpoint: endpoint.id.clone(),
                host: endpoint.label.clone(),
                epoch: self.selection_epoch,
                generation: endpoint.generation,
                boot: snapshot.boot_id.clone(),
                inbox: endpoint.connection.integrations.clone(),
            };
            let method = scope
                .inbox
                .try_lock()
                .map_err(|_| crate::Error::ConnectionBusy)?
                .list
                .ok_or(herdr_client::Error::UnsupportedMethod)?;
            let id = endpoint.connection.request_integration(
                &scope.boot,
                method,
                serde_json::json!({}),
            )?;
            Ok::<_, crate::Error>((scope, id, method))
        })();
        match request {
            Ok((scope, id, method)) => {
                self.integrations.scope = Some(scope);
                self.integrations.pending = Some((id, Operation::List(method)));
                self.integrations.busy = true;
            }
            Err(error) => self.integrations.error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Only call from an explicit install/reinstall click, never panel opening.
    pub(crate) fn install_integration(&mut self, target: String, cx: &mut Context<Self>) {
        if self.integrations.busy {
            return;
        }
        let Some(scope) = &self.integrations.scope else {
            return;
        };
        if !self.integration_scope_current(scope)
            || !self
                .integrations
                .rows
                .iter()
                .any(|row| row.target == target && row.available)
        {
            self.integrations.error = Some(fl!("integrations-list-changed"));
            cx.notify();
            return;
        }
        self.integrations.error = None;
        self.integrations.messages.clear();
        let method = match scope.inbox.try_lock() {
            Ok(inbox) => inbox.install,
            Err(_) => {
                self.integrations.error = Some(crate::Error::ConnectionBusy.to_string());
                cx.notify();
                return;
            }
        };
        let Some(method) = method else {
            self.integrations.error = Some(herdr_client::Error::UnsupportedMethod.to_string());
            cx.notify();
            return;
        };
        match self.endpoints[self.selected_endpoint]
            .connection
            .request_integration(
                &scope.boot,
                method,
                serde_json::json!({"target": target.clone()}),
            ) {
            Ok(id) => {
                self.integrations.pending = Some((id, Operation::Install { target, method }));
                self.integrations.busy = true;
            }
            Err(error) => self.integrations.error = Some(error.to_string()),
        }
        cx.notify();
    }

    /// Run after endpoint polling, including while the panel is closed.
    pub(crate) fn poll_integrations(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = &self.integrations.scope else {
            return;
        };
        if !self.integration_scope_current(scope) {
            self.integrations.clear_pending();
            self.integrations.scope = None;
            self.integrations.rows.clear();
            self.integrations.messages.clear();
            self.integrations.error = Some(fl!("integrations-host-changed"));
            cx.notify();
            return;
        }
        let Some((id, operation)) = &self.integrations.pending else {
            return;
        };
        let response = scope.inbox.try_lock().ok().and_then(|mut inbox| {
            let (request, response) = inbox.pending.as_mut()?;
            (request == id).then(|| response.take()).flatten()
        });
        let Some(response) = response else { return };
        let operation = operation.clone();
        self.integrations.clear_pending();
        match response {
            Err(error) => self.integrations.error = Some(error.to_string()),
            Ok(response) => match operation {
                Operation::List(method) => match parse_list(&response, method) {
                    Ok(rows) => self.integrations.rows = rows,
                    Err(error) => self.integrations.error = Some(error.to_string()),
                },
                Operation::Install { target, method } => {
                    match parse_install(&response, &target, method) {
                        Ok(messages) => {
                            self.load_integrations(cx);
                            self.integrations.messages = messages;
                        }
                        Err(error) => self.integrations.error = Some(error.to_string()),
                    }
                }
            },
        }
        cx.notify();
    }

    pub(crate) fn render_filtered_integrations(&self, query: &str, cx: &mut Context<Self>) -> Div {
        let query = query.trim().to_lowercase();
        let state = &self.integrations;
        let theme = &self.theme;
        let current = state
            .scope
            .as_ref()
            .filter(|scope| self.integration_scope_current(scope));
        let host = current
            .map(|scope| scope.host.clone())
            .or_else(|| {
                self.endpoints
                    .get(self.selected_endpoint)
                    .map(|endpoint| endpoint.label.clone())
            })
            .unwrap_or_else(|| fl!("integrations-no-host"));
        let can_install = !state.busy
            && current.is_some_and(|scope| {
                scope
                    .inbox
                    .try_lock()
                    .is_ok_and(|inbox| inbox.install.is_some())
            });
        let epoch = self.selection_epoch;
        let mut body = div()
            .id("integrations-body")
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .p(px(16.))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(crate::sidebar::label_text(&fl!(
                        "integrations-host",
                        host = host
                    ))),
            )
            .child(
                div()
                    .mt(px(8.))
                    .text_color(rgb(theme.subtext()))
                    .child(fl!("integrations-explainer")),
            );
        if let Some(error) = &state.error {
            body = body.child(
                div()
                    .mt(px(12.))
                    .text_color(rgb(theme.palette[1]))
                    .child(error.clone()),
            );
        }
        if state.busy {
            body = body.child(div().mt(px(12.)).child(fl!("integrations-working")));
        } else {
            body = body.child(
                div()
                    .id("integrations-refresh")
                    .mt(px(12.))
                    .cursor_pointer()
                    .child(fl!("integrations-refresh"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.selection_epoch == epoch {
                            this.load_integrations(cx);
                        }
                    })),
            );
        }
        if let Some(scope) = current {
            if !state.busy && state.rows.is_empty() && state.error.is_none() {
                body = body.child(div().mt(px(12.)).child(fl!("integrations-none")));
            }
            let rows: Vec<_> = state
                .rows
                .iter()
                .filter(|row| row.matches(&query))
                .collect();
            if rows.is_empty() && !state.rows.is_empty() {
                body = body.child(div().mt(px(12.)).child(fl!("integrations-no-match")));
            }
            for row in rows {
                let status = match row.state {
                    State::NotInstalled => fl!("integrations-not-installed"),
                    State::Current => fl!("integrations-current"),
                    State::Outdated => fl!("integrations-outdated"),
                };
                let mut item = div()
                    .py(px(12.))
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(row.label.clone()),
                    )
                    .child(div().text_color(rgb(theme.muted)).child(fl!(
                        "integrations-row-detail",
                        status = status,
                        command = row.command.as_str()
                    )));
                if !row.available {
                    item = item.child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(fl!("integrations-unavailable")),
                    );
                } else if can_install {
                    let target = row.target.clone();
                    let scope = scope.clone();
                    item = item.child(
                        div()
                            .id(SharedString::from(format!("install-{target}")))
                            .mt(px(6.))
                            .cursor_pointer()
                            .child(if row.state == State::NotInstalled {
                                fl!("integrations-install")
                            } else {
                                fl!("integrations-reinstall")
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this.integration_scope_current(&scope) {
                                    this.install_integration(target.clone(), cx);
                                }
                            })),
                    );
                } else if !state.busy {
                    item = item.child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(fl!("integrations-install-unsupported")),
                    );
                }
                body = body.child(item);
            }
            for message in &state.messages {
                body = body.child(div().mt(px(8.)).child(message.clone()));
            }
        }
        div().flex().flex_col().min_h_0().child(body)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use serde_json::json;

    fn list() -> Value {
        json!({"result":{"type":"integration_list","integrations":[{
            // i18n: data
            "target":"antigravity_cli","label":"Antigravity CLI","command":"antigravity",
            "available":true,"state":"outdated"
        }]}})
    }

    #[test]
    fn list_sort_and_search_keep_original_action_targets() {
        let response = json!({"result":{"type":"integration_list","integrations":[
            {"target":"zebra","label":"alpha","command":"z","available":true,"state":"current"},
            // i18n: data
            {"target":"beta","label":"Beta","command":"b","available":true,"state":"current"},
            {"target":"alpha","label":"ALPHA","command":"a","available":true,"state":"current"}
        ]}});
        let rows = parse_list(&response, Method::IntegrationList).unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.target.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zebra", "beta"]
        );
        assert_eq!(
            rows.iter()
                .filter(|row| row.matches("alpha"))
                .map(|row| row.target.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zebra"]
        );
        assert!(rows[1].matches("zebra"));
        assert!(!rows[1].matches("missing"));
        assert!(rows.iter().all(|row| row.matches("")));
    }

    #[test]
    fn list_parses_upstream_states_and_targets() {
        let mut response = list();
        for (wire, state) in [
            ("current", State::Current),
            ("outdated", State::Outdated),
            ("not_installed", State::NotInstalled),
        ] {
            response["result"]["integrations"][0]["state"] = json!(wire);
            let rows = parse_list(&response, Method::IntegrationList).unwrap();
            assert_eq!(rows[0].state, state);
            assert_eq!(rows[0].target, "antigravity_cli");
        }
    }

    #[test]
    fn rejects_malformed_duplicate_and_oversized_lists_atomically() {
        for (field, value) in [
            ("state", json!("future")),
            ("available", json!("true")),
            ("label", json!("x".repeat(257))),
            ("command", json!("bad\u{1b}command")),
            ("target", json!("../claude")),
        ] {
            let mut response = list();
            response["result"]["integrations"][0][field] = value;
            assert!(parse_list(&response, Method::IntegrationList).is_err());
        }
        let mut response = list();
        let row = response["result"]["integrations"][0].clone();
        response["result"]["integrations"] = json!([row.clone(), row.clone()]);
        assert!(matches!(
            parse_list(&response, Method::IntegrationList),
            Err(ResponseError::Invalid)
        ));
        response["result"]["integrations"] = json!(vec![row; MAX_ROWS + 1]);
        assert!(matches!(
            parse_list(&response, Method::IntegrationList),
            Err(ResponseError::Limit)
        ));
        assert!(
            parse_list(
                &json!({"result": {"integrations": []}}),
                Method::IntegrationList
            )
            .is_err()
        );
    }

    #[test]
    fn install_requires_matching_target_and_bounded_messages() {
        // i18n: data
        let mut response = json!({"result":{"type":"integration_install","target":"claude","details":{"messages":["Installed"]}}});
        assert_eq!(
            parse_install(&response, "claude", Method::IntegrationInstall).unwrap(),
            ["Installed"]
        );
        assert!(parse_install(&response, "codex", Method::IntegrationInstall).is_err());
        response["result"]["details"]["messages"] = json!(vec!["x"; MAX_MESSAGES + 1]);
        assert!(matches!(
            parse_install(&response, "claude", Method::IntegrationInstall),
            Err(ResponseError::Limit)
        ));
        response["result"]["details"]["messages"] = json!(["x".repeat(MAX_TEXT + 1)]);
        assert!(matches!(
            parse_install(&response, "claude", Method::IntegrationInstall),
            Err(ResponseError::Limit)
        ));
    }

    #[test]
    fn daemon_errors_are_validated_before_display() {
        let response =
            // i18n: data
            json!({"error":{"code":"integration_install_failed","message":"Permission denied"}});
        assert!(matches!(
            parse_list(&response, Method::IntegrationList),
            Err(ResponseError::Daemon { .. })
        ));
        let response = json!({"error":{"code":"failure","message":"x".repeat(MAX_TEXT + 1)}});
        assert!(matches!(
            parse_list(&response, Method::IntegrationList),
            Err(ResponseError::Limit)
        ));
    }

    #[test]
    fn v2_list_parses_omo_target_and_searches_by_label_or_target() {
        let response = json!({"result":{"type":"integration_list_v2","integrations":[{
            // i18n: data
            "target":"omo","label":"Omo Coding Agent","command":"omo","available":true,"state":"not_installed"
        }]}});
        let rows = parse_list(&response, Method::IntegrationListV2).unwrap();
        assert_eq!(rows[0].target, "omo");
        assert!(rows[0].matches("omo"));
        assert!(rows[0].matches("coding"));
        assert!(!rows[0].matches("claude"));
    }

    #[test]
    fn v2_accepts_legacy_cli_target_spelling_without_widening_v1() {
        let mut response = json!({"result":{"type":"integration_list_v2","integrations":[{
            // i18n: data
            "target":"antigravity-cli","label":"Antigravity","command":"agy","available":true,"state":"current"
        }]}});
        let rows = parse_list(&response, Method::IntegrationListV2).unwrap();
        assert_eq!(rows[0].target, "antigravity-cli");
        response["result"]["type"] = json!("integration_list");
        assert!(parse_list(&response, Method::IntegrationList).is_err());
    }

    #[test]
    fn v2_omo_target_survives_explicit_install_and_reloaded_list() {
        let response = json!({"result":{"type":"integration_install_v2","target":"omo",
            // i18n: data
            "details":{"messages":["Installed"]}}});
        let operation = Operation::Install {
            target: "omo".into(),
            method: Method::IntegrationInstallV2,
        };
        let Operation::Install { target, method } = operation else {
            unreachable!()
        };
        assert_eq!(
            parse_install(&response, &target, method).unwrap(),
            ["Installed"]
        );
        assert_eq!(target, "omo");
        let reloaded = json!({"result":{"type":"integration_list_v2","integrations":[{
                    // i18n: data
                    "target":"omo","label":"Omo","command"
        :"omo","available":true,"state":"current"
                }]}});
        let rows = parse_list(&reloaded, Method::IntegrationListV2).unwrap();
        assert_eq!(rows[0].target, target);
    }
}
