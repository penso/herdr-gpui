//! The window's side of chat panes: attaching to the panes its daemon
//! reports, moving the keyboard between a chat's composer and the terminal
//! as Herdr's focus moves, and sending what the user asks.

use super::{
    chats::{self, ChatInput, Connect},
    discovery,
    visual::{DiagramStyle, Output, VisualId},
};
use crate::{HerdrWindow, search_input::SearchInput};
use gpui::{prelude::*, *};
use herdr_chat_protocol::{Attachment, Command, MAX_ATTACHMENTS, MAX_PROMPT_SIZE, Status, assist};
use std::time::Instant;

impl HerdrWindow {
    /// Runs every tick: attaches to the selected daemon's chat panes when it
    /// is local, applies what their programs sent, and keeps each composer
    /// styled and holding the keyboard while its pane is focused.
    pub(crate) fn poll_chats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let program = self.chats.program.clone();
        self.poll_chats_with(
            &mut |target, since, resend| chats::connect(target, &program, since, resend),
            window,
            cx,
        );
    }

    /// [`Self::poll_chats`], connecting through `connect`.
    pub(super) fn poll_chats_with(
        &mut self,
        connect: &mut Connect<'_>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Off, every chat is let go and its pane shows its terminal again.
        let enabled = self.config.features.acp_chats;
        let route = self
            .endpoints
            .get(self.selected_endpoint)
            .filter(|_| enabled)
            .and_then(|endpoint| discovery::route(&endpoint.connection.target, &self.live));
        let now = Instant::now();
        let scope = self
            .endpoints
            .get(self.selected_endpoint)
            .filter(|_| self.config.chat.render_existing && route.is_some())
            .and_then(|endpoint| {
                self.live
                    .snapshot
                    .as_ref()
                    .map(|snapshot| super::existing::Scope {
                        target: endpoint.connection.target.clone(),
                        boot: snapshot.boot_id.clone(),
                        selection: self.selection_epoch,
                        connection: endpoint.generation,
                    })
            });
        let agents = self
            .live
            .snapshot
            .as_deref()
            .map(super::existing::candidates)
            .unwrap_or_default();
        let candidates: Vec<_> = agents.iter().map(|agent| agent.pane_id.clone()).collect();
        // A new conversation usually moves its pane's reported state, so the
        // daemon's change counters bring the next discovery probe forward.
        let activity = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for agent in &agents {
                (&agent.pane_id, &agent.agent, agent.state_change_seq).hash(&mut hasher);
            }
            hasher.finish()
        };
        let discovered = self.chats.existing.poll(scope, &candidates, activity, now);
        if discovered && let Some(error) = self.chats.existing.error() {
            tracing::debug!(%error, "Existing agent discovery unavailable");
        }
        let targets = self
            .live
            .snapshot
            .as_deref()
            .map(|snapshot| {
                discovery::with_mirrors(
                    discovery::with_existing(
                        discovery::targets(snapshot, route.as_ref()),
                        snapshot,
                        route.as_ref(),
                        self.chats.existing.bindings(),
                    ),
                    snapshot,
                    route.as_ref(),
                    &self.config.chat.mirrors,
                    self.config.chat.render_existing,
                )
            })
            .unwrap_or_default();
        if enabled {
            self.locate_chat_program(cx);
            self.probe_chat_agents(cx);
        }
        self.poll_chat_start(cx);
        let mut changed = self.chats.reconcile(targets, now, connect) || discovered;
        // A layout saved from Settings or the command line arrives here.
        changed |= self.chats.relayout(self.config.chat.layout);
        changed |= self.chats.rewidth(self.config.chat.max_width);
        changed |= self.chats.poll(now);
        // Observed panes, whether a discovered agent or a `[chat.mirrors]`
        // entry, take their status from the daemon; live chats ignore it.
        for pane in self.chats.panes_mut() {
            use herdr_client::protocol::AgentStatus;
            let status = match self.live.agent_status(&pane.target.pane_id) {
                Some(AgentStatus::Idle | AgentStatus::Done) => Status::Idle,
                Some(AgentStatus::Working | AgentStatus::Blocked) => Status::Working,
                _ => Status::Starting,
            };
            changed |= pane.session.set_observed_status(status);
        }
        self.prepare_chat_visuals(window, cx);
        let font = self.chat_font();
        let theme = self.theme.clone();
        for pane in self.chats.panes_mut() {
            if !pane.available() {
                continue;
            }
            pane.text_focus.get_or_insert_with(|| {
                changed = true;
                cx.focus_handle()
            });
            match pane.input() {
                ChatInput::Composer => {}
                // An observed live agent still owns its original terminal
                // editor below the rich conversation. It needs no second
                // composer and must retain normal terminal focus and IME.
                ChatInput::Terminal => {
                    pane.read_only_focus = None;
                    continue;
                }
                ChatInput::ReadOnly => {
                    pane.read_only_focus.get_or_insert_with(|| {
                        changed = true;
                        cx.focus_handle()
                    });
                    continue;
                }
            }
            let composer = pane.composer.get_or_insert_with(|| {
                changed = true;
                cx.new(|cx| {
                    let mut input = SearchInput::new(cx);
                    input.set_frameless(cx);
                    input
                })
            });
            let accepts = pane.session.transcript().status.accepts_prompts();
            // The selected command shows as ghost text after the typed name.
            let text = composer.read(cx).text().to_owned();
            let found = assist::completions(pane.session.transcript(), &text);
            if pane.completion >= found.len() {
                pane.completion = 0;
            }
            let ghost = found
                .get(pane.completion)
                .and_then(|command| command.name.strip_prefix(text.get(1..).unwrap_or("")))
                .map(str::to_owned);
            composer.update(cx, |input, cx| {
                input.set_completion(ghost, cx);
                input.set_appearance(font.clone(), theme.clone(), cx);
                input.set_placeholder(
                    if accepts {
                        "Message the agent \u{2014} / for commands, Shift-Enter for a new line"
                    } else {
                        "The agent cannot take prompts"
                    },
                    cx,
                );
            });
        }
        // A running turn's timer moves once a second.
        let second = super::now_ms() / 1000;
        if self
            .chats
            .panes()
            .any(|pane| pane.session.transcript().turn.is_some())
            && self.chats.ticked != second
        {
            self.chats.ticked = second;
            changed = true;
        }
        self.follow_chat_focus(window, cx);
        // The views are drawn with the terminal, so a surface redraw shows
        // them without rebuilding the cached sidebar.
        if changed {
            self.redraw_terminal(cx);
        }
    }

    /// Starts the pictures and diagrams each chat waits for on the
    /// background executor, and drops the textures no entry draws any more.
    pub(super) fn prepare_chat_visuals(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for texture in self.chats.take_released() {
            cx.drop_image(texture, Some(window));
        }
        #[cfg(any(target_os = "macos", windows))]
        for page in self.chats.take_closed_pages() {
            self.close_chat_page(page);
        }
        let font = self.chat_font();
        let style = DiagramStyle::of(&self.theme, &font);
        let renderer = cx.svg_renderer();
        for pane in self.chats.panes_mut() {
            for texture in pane.prepared.visuals.take_released() {
                cx.drop_image(texture, Some(window));
            }
            for job in pane.prepared.visuals.jobs(&style, &renderer) {
                let pane_id = pane.target.pane_id.clone();
                let task = cx.background_spawn(async move { job.run() });
                cx.spawn(async move |this, cx| {
                    let (id, ticket, drawn) = task.await;
                    let _ = this.update(cx, |this, cx| {
                        this.finish_chat_visual(&pane_id, id, ticket, drawn, cx);
                    });
                })
                .detach();
            }
        }
    }

    fn finish_chat_visual(
        &mut self,
        pane_id: &str,
        id: VisualId,
        ticket: u64,
        drawn: super::visual::Result<Output>,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if !pane.prepared.visuals.finish(id, ticket, drawn) {
            return;
        }
        // The row's height changed with what it now draws.
        let entries: Vec<_> = pane.prepared.drawing(id).collect();
        for entry in entries {
            pane.remeasure(entry);
        }
        self.redraw_terminal(cx);
    }

    /// The keyboard destination of the visible, Herdr-focused chat. A mirror
    /// uses an inert focus target so its hidden terminal cannot receive input.
    fn focused_chat_handle(&self, window: &Window, cx: &App) -> Option<FocusHandle> {
        let surface = self.live.surface.as_deref()?;
        if surface.popup.is_some() || !self.live.surface_ready() {
            return None;
        }
        let pane_id = self.live.snapshot.as_ref()?.focused_pane_id.as_deref()?;
        if !self.chats.shown(pane_id) || !surface.panes.iter().any(|p| p.pane_id == pane_id) {
            return None;
        }
        let pane = self.chats.pane(pane_id)?;
        if pane.reading(window) {
            return pane.text_focus.clone();
        }
        pane.input_focus(cx)
    }

    /// A visible chat owns the keyboard through its composer or inert mirror
    /// focus. Give it back once Herdr's focus moves on or a popup covers it.
    fn follow_chat_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.focused_chat_handle(window, cx);
        let holding = self.chats.panes().find_map(|pane| {
            pane.focus_handles(cx)
                .find(|focus| focus.is_focused(window))
        });
        match (holding, target) {
            (Some(holding), target) if target.as_ref() != Some(&holding) => {
                let focus = target.unwrap_or_else(|| self.focus.clone());
                window.focus(&focus, cx);
            }
            (None, Some(target))
                if self.focus.is_focused(window)
                    && self.menu.page.is_none()
                    && !self.copy_mode_active() =>
            {
                window.focus(&target, cx);
            }
            _ => {}
        }
    }

    /// Whether a chat owns the keyboard, so its hidden terminal cannot act
    /// on a keystroke bubbling out of the overlay or one of its fields.
    pub(crate) fn chat_focused(&self, window: &Window, cx: &App) -> bool {
        self.chats.panes().any(|pane| {
            pane.focus_handles(cx).any(|focus| focus.is_focused(window))
                || pane
                    .menu_search
                    .as_ref()
                    .is_some_and(|(search, _)| search.read(cx).focus.is_focused(window))
                || pane.widgets.focused(window, cx)
        })
    }

    /// Browser-style zoom while reading a rich conversation or typing in its
    /// original terminal editor. Other terminal panes keep their Ctrl keys,
    /// so these are fixed aliases beside the commands' keymap shortcuts.
    pub(crate) fn chat_zoom_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = &event.keystroke;
        let modifiers = key.modifiers;
        if !crate::controls::CTRL_SHORTCUTS
            || !modifiers.control
            || modifiers.alt
            || modifiers.platform
            || modifiers.function
            || event.prefer_character_input
        {
            return;
        }
        let command = match key.key.as_str() {
            "=" | "+" => crate::controls::Command::IncreaseFontSize,
            "-" if !modifiers.shift => crate::controls::Command::DecreaseFontSize,
            "0" if !modifiers.shift => crate::controls::Command::ResetFontSize,
            _ => return,
        };
        if self.menu.page.is_some()
            || self.copy_mode_active()
            || !(self.focus.is_focused(window) || self.chat_focused(window, cx))
            || self.keymap().pane_key(key).is_some()
        {
            return;
        }
        let Some(pane_id) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_pane_id.as_deref())
            .filter(|pane_id| self.chats.shown(pane_id))
        else {
            return;
        };
        if !self.live.surface.as_ref().is_some_and(|surface| {
            surface.popup.is_none() && surface.panes.iter().any(|pane| pane.pane_id == pane_id)
        }) {
            return;
        }
        self.command(command, window, cx);
        cx.stop_propagation();
        window.prevent_default();
    }

    /// Enter in a menu's search field picks the first match; Escape
    /// closes the menu.
    fn chat_menu_search_key(
        &mut self,
        pane_id: &str,
        search: &Entity<SearchInput>,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if search.read(cx).is_composing() || event.keystroke.modifiers.modified() {
            return;
        }
        let Some(pane) = self.chats.pane(pane_id) else {
            return;
        };
        let Some(menu) = pane.menu.clone() else {
            return;
        };
        match event.keystroke.key.as_str() {
            "enter" => {
                let query = search.read(cx).text().to_owned();
                let picked = match &menu {
                    chats::Menu::Setting(id) => pane
                        .session
                        .transcript()
                        .options
                        .iter()
                        .find(|option| &option.id == id),
                    _ => None,
                }
                .and_then(|option| match &option.kind {
                    herdr_chat_protocol::OptionKind::Select { choices, .. } => {
                        super::view::matching(choices, &query)
                            .next()
                            .map(|(_, choice)| Command::SetOption {
                                id: option.id.clone(),
                                value: herdr_chat_protocol::OptionValue::Choice {
                                    id: choice.id.clone(),
                                },
                            })
                    }
                    herdr_chat_protocol::OptionKind::Toggle { .. } => None,
                });
                self.toggle_chat_menu(pane_id, menu, window, cx);
                if let Some(command) = picked {
                    self.chat_send(pane_id, command, cx);
                }
            }
            "escape" => self.toggle_chat_menu(pane_id, menu, window, cx),
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }

    /// Sends `command` to the chat over `pane_id`; a failure shows in the view.
    pub(super) fn chat_send(&mut self, pane_id: &str, command: Command, cx: &mut Context<Self>) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        pane.error = pane.send(command).err().map(|error| error.to_string());
        cx.notify();
    }

    /// Enter sends the draft, Shift-Enter starts a new line of it, and
    /// Escape cancels a running turn.
    pub(super) fn chat_key_down(
        &mut self,
        pane_id: &str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_reader_key(pane_id, event, window, cx) {
            return;
        }
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane.widgets.focused(window, cx) {
            return;
        }
        match pane.input() {
            ChatInput::Composer => {}
            ChatInput::ReadOnly => {
                cx.stop_propagation();
                window.prevent_default();
                return;
            }
            ChatInput::Terminal => return,
        }
        if let Some(search) = pane
            .menu_search
            .as_ref()
            .map(|(search, _)| search.clone())
            .filter(|search| search.read(cx).focus.is_focused(window))
        {
            self.chat_menu_search_key(pane_id, &search, event, window, cx);
            return;
        }
        let Some(input) = pane.composer.clone() else {
            return;
        };
        if input.read(cx).is_composing() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        let shift_only =
            modifiers.shift && !modifiers.control && !modifiers.alt && !modifiers.platform;
        match event.keystroke.key.as_str() {
            "enter" if shift_only => {
                let line = input.read(cx).text().to_owned();
                pane.draft.push(line);
                input.update(cx, |input, cx| input.clear(cx));
            }
            "enter" if !modifiers.modified() => self.submit_chat_prompt(pane_id, &input, cx),
            "escape"
                if !modifiers.modified() && pane.session.transcript().status == Status::Working =>
            {
                self.chat_send(pane_id, Command::Cancel, cx);
            }
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    /// Keys the composer would otherwise take: Tab and Up/Down choose a
    /// slash command while some match, Up/Down recall earlier prompts from an
    /// empty composer, and Backspace there takes back the draft's last line.
    pub(super) fn chat_capture_key(
        &mut self,
        pane_id: &str,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_reader_key(pane_id, event, window, cx) {
            return;
        }
        if self.chats.pane(pane_id).is_some_and(|pane| {
            pane.widgets.focused(window, cx) || pane.input() != ChatInput::Composer
        }) {
            return;
        }
        if crate::actions::edit_modifiers(event.keystroke.modifiers)
            && event.keystroke.key == "v"
            && self.chat_paste(pane_id, window, cx)
        {
            cx.stop_propagation();
            window.prevent_default();
            return;
        }
        if event.keystroke.modifiers.modified() {
            return;
        }
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        // The menu's search field takes its own keys.
        if pane
            .menu_search
            .as_ref()
            .is_some_and(|(search, _)| search.read(cx).focus.is_focused(window))
        {
            return;
        }
        let Some(input) = pane.composer.clone() else {
            return;
        };
        if input.read(cx).is_composing() {
            return;
        }
        let text = input.read(cx).text().to_owned();
        let found = assist::completions(pane.session.transcript(), &text).len();
        let key = event.keystroke.key.as_str();
        match key {
            "tab" if found > 0 => {
                let Some(name) = assist::completions(pane.session.transcript(), &text)
                    .get(pane.completion)
                    .map(|command| command.name.clone())
                else {
                    return;
                };
                self.complete_chat_command(pane_id, &name, window, cx);
            }
            "up" | "down" if found > 0 => {
                pane.completion = if key == "up" {
                    pane.completion.saturating_sub(1)
                } else {
                    (pane.completion + 1).min(found - 1)
                };
            }
            "up" | "down" if text.is_empty() || pane.history.recalling() => {
                let recalled = if key == "up" {
                    pane.history.older(pane.session.transcript())
                } else {
                    pane.history.newer()
                };
                let Some(recalled) = recalled.map(str::to_owned) else {
                    return;
                };
                self.set_chat_prompt(pane_id, &input, &recalled, window, cx);
            }
            "backspace" if text.is_empty() => {
                let Some(line) = pane.draft.pop() else {
                    return;
                };
                input.update(cx, |input, cx| {
                    input.replace_text_in_range(None, &line, window, cx);
                });
            }
            _ => {
                if key != "up" && key != "down" {
                    pane.history.stop();
                }
                return;
            }
        }
        cx.stop_propagation();
        window.prevent_default();
        cx.notify();
    }

    /// Pastes the clipboard's text into the composer when it holds several
    /// lines, which the one-line composer would join: every line but the
    /// last joins the draft above it. Whether it took the paste.
    pub(super) fn chat_paste(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.chat_reader_focused(pane_id, window) {
            return true;
        }
        // An inert mirror focus still belongs to the chat. Consume Paste so
        // the terminal's action handler cannot paste beneath the overlay.
        if self.chats.pane(pane_id).is_some_and(|pane| {
            pane.input() == ChatInput::ReadOnly
                && pane
                    .input_focus(cx)
                    .is_some_and(|focus| focus.is_focused(window))
        }) {
            return true;
        }
        let Some(input) = self
            .chats
            .pane(pane_id)
            .and_then(|pane| pane.composer.clone())
        else {
            return false;
        };
        if !input.read(cx).focus.is_focused(window) {
            return false;
        }
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return false;
        };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        if !text.contains('\n') {
            return false;
        }
        // The composer's selection is not exposed, so a marker it keeps (it
        // drops only line breaks) stands in for the paste and shows where
        // the line splits around it.
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, PASTE_MARK, window, cx);
        });
        let line = input.read(cx).text().to_owned();
        let (before, after) = line.split_once(PASTE_MARK).unwrap_or((line.as_str(), ""));
        let joined = format!("{before}{text}{after}");
        let (lines, last) = draft_lines(&joined);
        let last = last.to_owned();
        // The cursor goes right after the pasted text, before the line's tail.
        let cursor = last.len() - after.len();
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return false;
        };
        pane.draft.extend(lines);
        input.update(cx, |input, cx| input.set_text_at(&last, cursor, cx));
        self.redraw_terminal(cx);
        cx.notify();
        true
    }

    /// Puts `text` in the composer: its last line in the editable field,
    /// the lines before it in the draft, replacing both.
    fn set_chat_prompt(
        &mut self,
        pane_id: &str,
        input: &Entity<SearchInput>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let (draft, last) = draft_lines(text);
        pane.draft = draft;
        input.update(cx, |input, cx| {
            input.clear(cx);
            input.replace_text_in_range(None, last, window, cx);
        });
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// Replaces the composer's text with `/name `, ready for its input.
    pub(super) fn complete_chat_command(
        &mut self,
        pane_id: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        pane.completion = 0;
        let Some(input) = pane.composer.clone() else {
            return;
        };
        let text = format!("/{name} ");
        input.update(cx, |input, cx| {
            input.clear(cx);
            input.replace_text_in_range(None, &text, window, cx);
        });
        let focus = input.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Adds files to the next prompt of the chat over `pane_id`. A chat on
    /// an SSH host takes copies made there, since this machine's paths name
    /// nothing on that host.
    pub(crate) fn attach_chat_files(
        &mut self,
        pane_id: &str,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .chats
            .pane(pane_id)
            .is_some_and(|pane| pane.input() != ChatInput::Composer)
        {
            return;
        }
        let host = match self.chats.pane(pane_id).map(|pane| &pane.target.route) {
            Some(super::Route::Ssh(host)) => host.clone(),
            Some(super::Route::Local) => return self.add_chat_files(pane_id, paths, window, cx),
            None => return,
        };
        self.upload_chat_files(pane_id, host, paths, window, cx);
    }

    /// Copies files to `host` off the UI thread, then attaches the copies.
    fn upload_chat_files(
        &mut self,
        pane_id: &str,
        host: String,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        pane.uploading += paths.len();
        let count = paths.len();
        let upload = cx.background_spawn(async move {
            let cancelled = std::sync::atomic::AtomicBool::new(false);
            herdr_client::upload_files(&host, &paths, &cancelled, |_, _| {})
        });
        let pane_id = pane_id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let uploaded = upload.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(pane) = this.chats.pane_mut(&pane_id) else {
                    return;
                };
                pane.uploading = pane.uploading.saturating_sub(count);
                match uploaded {
                    Ok(remote) => {
                        let remote = remote.into_iter().map(std::path::PathBuf::from).collect();
                        this.add_chat_files(&pane_id, remote, window, cx);
                    }
                    Err(error) => {
                        pane.error = Some(format!("Files not attached: {error}"));
                        this.redraw_terminal(cx);
                    }
                }
            });
        })
        .detach();
        self.redraw_terminal(cx);
    }

    /// Adds paths, as the program will read them, to the next prompt,
    /// skipping ones already there and any past [`MAX_ATTACHMENTS`]. Whether
    /// each exists is checked by the program when the prompt goes out.
    fn add_chat_files(
        &mut self,
        pane_id: &str,
        paths: Vec<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        if pane.input() != ChatInput::Composer {
            return;
        }
        for path in paths {
            if pane.attachments.len() >= MAX_ATTACHMENTS {
                pane.error = Some(format!("A prompt takes at most {MAX_ATTACHMENTS} files"));
                break;
            }
            if !pane.attachments.contains(&path) {
                pane.attachments.push(path);
            }
        }
        if let Some(input) = pane.composer.clone() {
            let focus = input.read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        self.redraw_terminal(cx);
        cx.notify();
    }

    /// The system file dialog; the chosen files join the next prompt.
    pub(super) fn browse_chat_files(
        &mut self,
        pane_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach".into()),
        });
        let pane_id = pane_id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.attach_chat_files(&pane_id, paths, window, cx);
            });
        })
        .detach();
    }

    /// Takes a file back off the next prompt.
    pub(super) fn detach_chat_file(&mut self, pane_id: &str, index: usize, cx: &mut Context<Self>) {
        if let Some(pane) = self.chats.pane_mut(pane_id)
            && index < pane.attachments.len()
        {
            pane.attachments.remove(index);
            self.redraw_terminal(cx);
            cx.notify();
        }
    }

    /// Sends what the composer holds, as Enter does.
    pub(super) fn send_chat_composer(&mut self, pane_id: &str, cx: &mut Context<Self>) {
        let Some(input) = self
            .chats
            .pane(pane_id)
            .and_then(|pane| pane.composer.clone())
        else {
            return;
        };
        self.submit_chat_prompt(pane_id, &input, cx);
        cx.notify();
    }

    fn submit_chat_prompt(
        &mut self,
        pane_id: &str,
        input: &Entity<SearchInput>,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let text = prompt_text(&pane.draft, input.read(cx).text());
        if text.trim().is_empty() && pane.attachments.is_empty() && pane.uploading == 0 {
            return;
        }
        // Files still copying to an SSH host belong to this prompt; sending
        // now would leave them for the next one.
        if pane.uploading > 0 {
            pane.error = Some("Files are still being copied; send once they are attached".into());
            return;
        }
        if text.len() > MAX_PROMPT_SIZE {
            pane.error = Some("The prompt is longer than the agent accepts".into());
            return;
        }
        if !text.trim().is_empty() {
            pane.history.push(&text, pane.session.transcript());
        }
        let attachments = pane
            .attachments
            .iter()
            .map(|path| Attachment {
                path: path.to_string_lossy().into_owned(),
            })
            .collect();
        match pane.send(Command::Prompt { text, attachments }) {
            Ok(()) => {
                pane.error = None;
                pane.draft.clear();
                pane.attachments.clear();
                input.update(cx, |input, cx| input.clear(cx));
            }
            Err(error) => pane.error = Some(error.to_string()),
        }
    }

    /// Shows the terminal under the chat over `pane_id`, or the chat again;
    /// `false` when the pane has no chat.
    pub(crate) fn set_chat_raw(
        &mut self,
        pane_id: &str,
        raw: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.chats.set_raw(pane_id, raw) {
            return false;
        }
        if raw && self.chat_focused(window, cx) {
            window.focus(&self.focus, cx);
        }
        self.follow_chat_focus(window, cx);
        cx.notify();
        true
    }
}

/// What a paste briefly puts at the composer's cursor: private-use
/// characters no prompt is expected to hold.
const PASTE_MARK: &str = "\u{F8FF}\u{E000}";

/// `text` as draft lines and the composer's last line, so that
/// [`prompt_text`] rebuilds it.
pub(crate) fn draft_lines(text: &str) -> (Vec<String>, &str) {
    match text.rsplit_once('\n') {
        Some((head, last)) => (head.split('\n').map(str::to_owned).collect(), last),
        None => (Vec::new(), text),
    }
}

/// The prompt a draft and the composer's line make.
pub(crate) fn prompt_text(draft: &[String], line: &str) -> String {
    if draft.is_empty() {
        return line.to_owned();
    }
    let mut text = draft.join("\n");
    text.push('\n');
    text.push_str(line);
    text
}
