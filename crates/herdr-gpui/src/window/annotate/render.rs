// i18n: migrated
use super::*;
use crate::i18n::fl;
use crate::{config::corners, fonts::StyledFont};

/// Prefer just below the selection, flip above it at the bottom edge, and
/// clamp to the pane. Both the pill and composer use these same coordinates.
fn placement(
    pane: Bounds<Pixels>,
    anchor: Bounds<Pixels>,
    desired: Size<Pixels>,
) -> Bounds<Pixels> {
    let inset = px(8.).min(pane.size.width / 2.).min(pane.size.height / 2.);
    let width = desired
        .width
        .min((pane.size.width - inset * 2.).max(px(0.)));
    let height = desired
        .height
        .min((pane.size.height - inset * 2.).max(px(0.)));
    let left = (anchor.right() - width).clamp(pane.left() + inset, pane.right() - inset - width);
    let below = anchor.bottom() + px(6.);
    let top = if below + height <= pane.bottom() - inset {
        below
    } else {
        anchor.top() - px(6.) - height
    }
    .clamp(pane.top() + inset, pane.bottom() - inset - height);
    Bounds::new(point(left, top), size(width, height))
}

impl HerdrWindow {
    /// The only pointer-occluding area is the pill or the compact composer.
    /// The terminal keeps focus until the user explicitly chooses Annotate.
    pub(crate) fn render_terminal_note(
        &self,
        surface: Option<&herdr_client::protocol::PaneSurfaceFrame>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        if self.menu.page.is_some() {
            return None;
        }
        let surface = surface.filter(|surface| surface.popup.is_none())?;
        let composer = self.terminal_notes.composer.as_ref();
        let selection = match composer {
            Some(composer) => &composer.selection,
            None => {
                &self
                    .terminal_notes
                    .offer
                    .as_ref()
                    .filter(|offer| offer.current(self))?
                    .selection
            }
        };
        let pane_id = selection.pane_id()?;
        let pane = surface.panes.iter().find(|pane| pane.pane_id == pane_id)?;
        let cell_height = self.config.terminal.line_height();
        let pane_bounds = Bounds::new(
            point(
                px(f32::from(pane.rect.x) * self.cell_width),
                px(f32::from(pane.rect.y) * cell_height),
            ),
            size(
                px(f32::from(pane.rect.width) * self.cell_width),
                px(f32::from(pane.rect.height) * cell_height),
            ),
        );
        let anchor = selection
            .rows(surface, self.cell_width, cell_height)
            .last()
            .map(|(row, columns)| {
                Bounds::from_corners(
                    point(
                        px(f32::from(columns.start) * self.cell_width),
                        px(f32::from(row) * cell_height),
                    ),
                    point(
                        px(f32::from(columns.end) * self.cell_width),
                        px((f32::from(row) + 1.) * cell_height),
                    ),
                )
            })
            .unwrap_or(Bounds::new(
                pane_bounds.origin,
                size(px(0.), px(cell_height)),
            ));
        let mut font = self.config.ui.clone();
        font.size = font.size.max(14.);
        let desired = if composer.is_some() {
            // 14px padding, two 12px gaps, 24px header, padded input, 32px footer.
            size(px(410.), px(132. + font.line_height()))
        } else {
            size(px(font.size * 6. + 48.), px(font.line_height() + 12.))
        };
        let bounds = placement(pane_bounds, anchor, desired);
        let theme = &self.theme;
        let content = match composer {
            Some(composer) => self.terminal_note_card(composer, cx),
            None => div()
                .id("terminal-annotate")
                .debug_selector(|| "terminal-annotate".into())
                .occlude()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .px_3()
                .rounded_full()
                .shadow_md()
                .bg(rgb(theme.surface))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(theme.active)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                .on_mouse_down_out(
                    cx.listener(|this, _, _, cx| this.clear_terminal_annotation_offer(cx)),
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_terminal_annotation_offer(window, cx)
                }))
                .child(
                    svg()
                        .path("icons/note.svg")
                        .flex_none()
                        .size(px(16.))
                        .text_color(rgb(theme.subtext())),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(fl!("window-annotate-offer")),
                ),
        };
        Some(
            div()
                .absolute()
                .left(bounds.left())
                .top(bounds.top())
                .w(bounds.size.width)
                .h(bounds.size.height)
                .text_font(&font)
                .text_size(px(font.size))
                .line_height(px(font.line_height()))
                .text_color(rgb(theme.foreground))
                .child(content),
        )
    }

    fn terminal_note_card(&self, composer: &Composer, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = &self.theme;
        let queued = self.terminal_notes.queued.len();
        let hint = if queued > 0 {
            fl!("window-annotate-hint-queued", count = queued)
        } else if composer.target.is_none() {
            fl!("window-annotate-hint-copies")
        } else {
            fl!("window-annotate-hint-send")
        };
        let button = |id: &'static str, label: String, primary: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .px_3()
                .py_1p5()
                .rounded_full()
                .cursor_pointer()
                .when(primary, |button| {
                    button
                        .bg(rgb(theme.primary()))
                        .text_color(rgb(theme.text_on(theme.primary())))
                })
                .when(!primary, |button| {
                    button
                        .text_color(rgb(theme.subtext()))
                        .hover(|s| s.bg(rgb(theme.active)))
                })
                .child(label)
        };
        div()
            .id("terminal-note")
            .debug_selector(|| "terminal-note".into())
            .occlude()
            .size_full()
            .overflow_y_scroll()
            .rounded(px(corners::CONTROL))
            .bg(rgb(theme.surface))
            .shadow_xl()
            .on_key_down(cx.listener(Self::terminal_note_key_down))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p(px(14.))
                    .child(
                        div()
                            .flex_none()
                            .h(px(24.))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                svg()
                                    .path("icons/note.svg")
                                    .flex_none()
                                    .size(px(16.))
                                    .text_color(rgb(theme.subtext())),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "terminal-note-title".into())
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(fl!("window-annotate-title")),
                            )
                            .child(
                                div()
                                    .id("terminal-note-cancel")
                                    .debug_selector(|| "terminal-note-cancel".into())
                                    .flex_none()
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded_full()
                                    .cursor_pointer()
                                    .hover(|style| style.bg(rgb(theme.active)))
                                    .child(
                                        svg()
                                            .path("icons/close.svg")
                                            .size(px(13.))
                                            .text_color(rgb(theme.subtext())),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_terminal_note(window, cx)
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .debug_selector(|| "terminal-note-input".into())
                            .flex_none()
                            .min_w_0()
                            .p_3()
                            .rounded(px(corners::CONTROL))
                            .bg(rgb(theme.background))
                            .child(composer.input.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .debug_selector(|| "terminal-note-hint".into())
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(11.))
                                    .text_color(rgb(theme.subtext()))
                                    .child(hint),
                            )
                            .child(
                                button("terminal-note-queue", fl!("window-annotate-queue"), false)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_terminal_note(false, window, cx)
                                    })),
                            )
                            .child(
                                button("terminal-note-send", fl!("window-annotate-send"), true)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.add_terminal_note(true, window, cx)
                                    })),
                            ),
                    ),
            )
    }
}
