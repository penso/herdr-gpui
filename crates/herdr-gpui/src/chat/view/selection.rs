//! Shaped text hit testing for one transcript. Copy order and source bytes
//! belong to the prepared document; this registry holds only mounted geometry.

use crate::{
    HerdrWindow,
    chat::selection::{Caret, Cell, Document, Key, Region, Segment, Selection},
    config::Theme,
};
use gpui::{prelude::*, *};
use herdr_chat_protocol::EntryId;
use std::{cell::RefCell, ops::Range, rc::Rc};

#[derive(Clone, Default)]
pub(crate) struct Geometry(Rc<RefCell<GeometryState>>);

#[derive(Default)]
struct GeometryState {
    frame: u64,
    targets: Vec<Hit>,
    gesture: Option<Gesture>,
}

struct Gesture {
    caret: Caret,
    origin: Point<Pixels>,
    moved: bool,
    activate: bool,
    dragging: bool,
}

#[derive(Clone)]
struct Hit {
    key: Key,
    range: Range<usize>,
    bounds: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    align: TextAlign,
    layout: Option<TextLayout>,
}

impl Hit {
    fn caret(&self, mut position: Point<Pixels>) -> Caret {
        // Overdraw rows are mounted outside the scroll viewport. Selecting
        // toward them stops at visible text until autoscroll reveals more.
        position.x = position.x.clamp(self.clip.left(), self.clip.right());
        position.y = position.y.clamp(
            self.clip.top(),
            (self.clip.bottom() - px(0.5)).max(self.clip.top()),
        );
        let offset = if let Some(layout) = &self.layout {
            position.x -= self.alignment_offset(position.y);
            let local = text_offset(layout, position);
            self.range.start + local.min(self.range.len())
        } else if position.x < self.bounds.center().x {
            self.range.start
        } else {
            self.range.end
        };
        Caret {
            key: self.key,
            offset,
        }
    }

    fn distance(&self, position: Point<Pixels>) -> f32 {
        let bounds = self.bounds.intersect(&self.clip);
        let x = f32::from(position.x);
        let y = f32::from(position.y);
        let dx = x - x.clamp(f32::from(bounds.left()), f32::from(bounds.right()));
        let dy = y - y.clamp(f32::from(bounds.top()), f32::from(bounds.bottom()));
        dx * dx + dy * dy
    }

    /// GPUI applies alignment during painting but its TextLayout hit testing
    /// uses unaligned coordinates. Mirror that offset for each wrapped row.
    fn alignment_offset(&self, y: Pixels) -> Pixels {
        let Some(layout) = &self.layout else {
            return px(0.);
        };
        if self.align == TextAlign::Left {
            return px(0.);
        }
        let bounds = layout.bounds();
        let mut row = ((y - bounds.top()).max(px(0.)) / layout.line_height()) as usize;
        for line in layout.line_layouts() {
            if row > line.wrap_boundaries.len() {
                row -= line.wrap_boundaries.len() + 1;
                continue;
            }
            let x = |index: usize| {
                let boundary = line.wrap_boundaries[index];
                line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix]
                    .position
                    .x
            };
            let start = if row == 0 { px(0.) } else { x(row - 1) };
            let end = if row == line.wrap_boundaries.len() {
                line.unwrapped_layout.width
            } else {
                x(row)
            };
            let remaining = bounds.size.width - (end - start);
            return if self.align == TextAlign::Center {
                remaining * 0.5
            } else {
                remaining
            };
        }
        px(0.)
    }
}

/// Text selection uses the nearest caret, unlike a link hit which asks which
/// glyph is underneath the pointer. In particular, roundoff at a shaped byte
/// boundary must not drop the final character from a copied selection.
fn text_offset(layout: &TextLayout, position: Point<Pixels>) -> usize {
    let mut origin = layout.bounds().origin;
    if position.y < origin.y {
        return 0;
    }
    let mut start = 0;
    for line in layout.line_layouts() {
        let bottom = origin.y + line.size(layout.line_height()).height;
        if position.y < bottom {
            let offset =
                match line.closest_index_for_position(position - origin, layout.line_height()) {
                    Ok(offset) | Err(offset) => offset,
                };
            return start + offset;
        }
        origin.y = bottom;
        start += line.len() + 1;
    }
    layout.len()
}

impl Geometry {
    /// Called before rendering the list, including its lazy visible-row callback.
    /// Old callbacks cannot repopulate a replacement frame's hit targets.
    pub(crate) fn begin_frame(&self) {
        let mut state = self.0.borrow_mut();
        state.frame = state.frame.wrapping_add(1);
        state.targets.clear();
    }

    pub(crate) fn clear(&self) {
        self.begin_frame();
        self.0.borrow_mut().gesture = None;
    }

    pub(crate) fn release(&self) {
        if let Some(gesture) = &mut self.0.borrow_mut().gesture {
            gesture.dragging = false;
        }
    }

    fn press(&self, caret: Caret, position: Point<Pixels>, clicks: usize, extend: bool) {
        self.0.borrow_mut().gesture = Some(Gesture {
            caret,
            origin: position,
            moved: false,
            activate: clicks == 1 && !extend,
            dragging: true,
        });
    }

    fn moved(&self, position: Point<Pixels>) {
        if let Some(gesture) = &mut self.0.borrow_mut().gesture
            && gesture.dragging
        {
            let delta = position - gesture.origin;
            gesture.moved |= f32::from(delta.x).abs() > 3. || f32::from(delta.y).abs() > 3.;
        }
    }

    fn dragged(&self, caret: Caret) {
        if let Some(gesture) = &mut self.0.borrow_mut().gesture
            && gesture.dragging
        {
            gesture.moved |= gesture.caret != caret;
        }
    }

    /// The closest mounted text, including when dragging through row gaps.
    /// The reader validates the returned revision key against the document.
    pub(crate) fn hit(&self, position: Point<Pixels>) -> Option<Caret> {
        self.moved(position);
        let caret = self
            .0
            .borrow()
            .targets
            .iter()
            .min_by(|a, b| a.distance(position).total_cmp(&b.distance(position)))
            .map(|target| target.caret(position));
        if let Some(caret) = caret {
            self.dragged(caret);
        }
        caret
    }

    /// A disclosure label remains clickable while dragging its text does not
    /// toggle the card. Called on mouse-up before the list releases the gesture.
    pub(crate) fn card_click(&self, entry: EntryId, position: Point<Pixels>) -> bool {
        let Some(caret) = self.hit(position) else {
            return false;
        };
        self.0.borrow().gesture.as_ref().is_some_and(|gesture| {
            gesture.dragging
                && gesture.activate
                && !gesture.moved
                && gesture.caret.key == caret.key
                && caret.key.entry() == entry
        })
    }

    #[cfg(any(test, feature = "integration-test"))]
    pub(crate) fn position(&self, key: Key, offset: usize) -> Option<Point<Pixels>> {
        self.0.borrow().targets.iter().find_map(|target| {
            if target.key != key || !target.range.contains(&offset) && target.range.end != offset {
                return None;
            }
            let position = if let Some(layout) = &target.layout {
                layout
                    .position_for_index(offset.saturating_sub(target.range.start))
                    .map(|position| {
                        let y = position.y + layout.line_height() * 0.5;
                        point(position.x + target.alignment_offset(y), y)
                    })
            } else {
                Some(point(
                    if offset <= target.range.start {
                        target.bounds.left()
                    } else {
                        target.bounds.right()
                    },
                    target.bounds.center().y,
                ))
            }?;
            // A right-aligned final caret lies on the element's exclusive
            // right edge. Return a point inside that same caret's hit region.
            let visible = target.bounds.intersect(&target.clip);
            Some(point(
                position.x.clamp(
                    visible.left(),
                    (visible.right() - px(0.5)).max(visible.left()),
                ),
                position.y.clamp(
                    visible.top(),
                    (visible.bottom() - px(0.5)).max(visible.top()),
                ),
            ))
        })
    }
}

pub(crate) struct Scope<'a> {
    pane: &'a str,
    entry: EntryId,
    region: Region,
    document: &'a Document,
    selection: &'a Selection,
    geometry: &'a Geometry,
}

impl<'a> Scope<'a> {
    pub(crate) fn new(
        pane: &'a str,
        entry: EntryId,
        region: Region,
        document: &'a Document,
        selection: &'a Selection,
        geometry: &'a Geometry,
    ) -> Self {
        Self {
            pane,
            entry,
            region,
            document,
            selection,
            geometry,
        }
    }

    pub(crate) fn fragment(&self, cell: Cell) -> Option<Fragment> {
        let key = self.document.key(self.entry, self.region, cell)?;
        let segment = self.document.segment(key)?;
        Some(Fragment {
            pane: self.pane.to_owned(),
            key,
            range: 0..segment.text().len(),
            selected: self.selection.highlight(self.document, key),
            backgrounds: None,
            geometry: self.geometry.clone(),
            frame: self.geometry.0.borrow().frame,
        })
    }

    pub(crate) fn segment(&self, cell: Cell) -> Option<&Segment> {
        self.document
            .segment(self.document.key(self.entry, self.region, cell)?)
    }

    pub(crate) fn text(&self, cell: Cell) -> Option<&str> {
        self.segment(cell).map(Segment::text)
    }
}

/// `text` with `marks`, and with the selection's highlight where it has a
/// fragment of the document.
pub(crate) fn styled_text(
    fragment: Option<&Fragment>,
    text: impl Into<SharedString>,
    marks: impl IntoIterator<Item = (Range<usize>, HighlightStyle)>,
    theme: &Theme,
) -> StyledText {
    match fragment {
        Some(fragment) => fragment.styled(text, marks, theme),
        None => StyledText::new(text).with_highlights(marks),
    }
}

/// `view`, selectable where it draws a fragment of the document.
pub(crate) fn selectable(
    fragment: Option<&Fragment>,
    view: Div,
    layout: TextLayout,
    cx: &mut Context<HerdrWindow>,
) -> Div {
    match fragment {
        Some(fragment) => fragment.wrap(view, layout, cx),
        None => view,
    }
}

#[derive(Clone)]
pub(crate) struct Fragment {
    pane: String,
    key: Key,
    range: Range<usize>,
    selected: Option<Range<usize>>,
    backgrounds: Option<Rc<RefCell<Vec<Background>>>>,
    geometry: Geometry,
    frame: u64,
}

impl Fragment {
    /// GPUI 0.3.6 positions aligned glyphs correctly, but ends their background
    /// runs at an unaligned x. Paint those marks from the same shaped rows.
    pub(crate) fn aligned(mut self) -> Self {
        self.backgrounds = Some(Rc::default());
        self
    }

    pub(crate) fn range(&self, range: Range<usize>) -> Self {
        let mut fragment = self.clone();
        let start = range.start.clamp(self.range.start, self.range.end);
        fragment.range = start..range.end.clamp(start, self.range.end);
        fragment.backgrounds = self.backgrounds.as_ref().map(|_| Rc::default());
        fragment
    }

    fn selected(&self) -> Option<Range<usize>> {
        let selected = self.selected.as_ref()?;
        let start = selected.start.max(self.range.start);
        let end = selected.end.min(self.range.end);
        (start < end).then(|| start - self.range.start..end - self.range.start)
    }

    /// Split at every style/selection edge; StyledText expects ordered,
    /// nonoverlapping highlights. Font family overrides stay on the caller.
    pub(crate) fn styled(
        &self,
        text: impl Into<SharedString>,
        marks: impl IntoIterator<Item = (Range<usize>, HighlightStyle)>,
        theme: &Theme,
    ) -> StyledText {
        let text = text.into();
        let mut marks = highlights(&text, marks.into_iter().collect(), self.selected(), theme);
        if let Some(backgrounds) = &self.backgrounds {
            *backgrounds.borrow_mut() = marks
                .iter_mut()
                .filter_map(|(range, style)| {
                    style.background_color.take().map(|color| Background {
                        range: range.clone(),
                        color,
                    })
                })
                .collect();
        }
        StyledText::new(text).with_highlights(marks)
    }

    pub(crate) fn allows_link(&self) -> bool {
        self.geometry
            .0
            .borrow()
            .gesture
            .as_ref()
            .is_some_and(|gesture| {
                gesture.caret.key == self.key && gesture.activate && !gesture.moved
            })
    }

    pub(crate) fn wrap(&self, view: Div, layout: TextLayout, cx: &mut Context<HerdrWindow>) -> Div {
        self.attach(view, Some(layout), cx)
    }

    pub(crate) fn atomic(&self, view: Div, theme: &Theme, cx: &mut Context<HerdrWindow>) -> Div {
        let selected = self.selected().is_some();
        let view = view
            .border_1()
            .border_color(if selected {
                rgb(theme.primary()).into()
            } else {
                Hsla::transparent_black()
            })
            .when(selected, |view| view.bg(rgb(theme.primary_wash())));
        self.attach(view, None, cx)
    }

    fn attach(&self, view: Div, layout: Option<TextLayout>, cx: &mut Context<HerdrWindow>) -> Div {
        let geometry = self.geometry.clone();
        let backgrounds = self
            .backgrounds
            .as_ref()
            .map(|marks| marks.borrow().clone());
        let target = Hit {
            key: self.key,
            range: self.range.clone(),
            bounds: Bounds::default(),
            clip: Bounds::default(),
            align: TextAlign::Left,
            layout,
        };
        let measured = Rc::new(RefCell::new(target));
        let registered = measured.clone();
        let frame = self.frame;
        let press = self.clone();
        let pressed = measured.clone();
        let drag = self.clone();
        view.relative()
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let caret = pressed.borrow().caret(event.position);
                    press.geometry.press(
                        caret,
                        event.position,
                        event.click_count,
                        event.modifiers.shift,
                    );
                    this.press_chat_text(
                        &press.pane,
                        caret,
                        event.click_count,
                        event.modifiers.shift,
                        window,
                        cx,
                    );
                }),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    drag.geometry.moved(event.position);
                    let caret = measured.borrow().caret(event.position);
                    drag.geometry.dragged(caret);
                    this.drag_chat_text(&drag.pane, caret, cx);
                }
            }))
            .child(
                canvas(
                    move |bounds, window, _| {
                        let clip = window.content_mask().bounds;
                        let mut target = registered.borrow_mut();
                        target.bounds = bounds;
                        target.clip = clip;
                        target.align = window.text_style().text_align;
                        target.clone()
                    },
                    move |_, target, window, _| {
                        if let (Some(layout), Some(backgrounds)) = (&target.layout, &backgrounds) {
                            for background in backgrounds {
                                for bounds in
                                    background_rects(layout, target.align, &background.range)
                                {
                                    window.paint_quad(fill(bounds, background.color));
                                }
                            }
                        }
                        // List prepaint may be rolled back and retried. Only
                        // committed paint has the mouse handlers corresponding
                        // to these coordinates; discarded layouts stay local.
                        let mut state = geometry.0.borrow_mut();
                        let visible = target.bounds.intersect(&target.clip);
                        if state.frame == frame
                            && visible.size.width > px(0.)
                            && visible.size.height > px(0.)
                        {
                            state.targets.push(target.clone());
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
    }
}

#[derive(Clone)]
struct Background {
    range: Range<usize>,
    color: Hsla,
}

/// One rectangle per selected wrapped row, including a partial final row.
/// Glyph positions supply the widths, so Unicode, shaping and font overrides
/// agree with the painted text. Alignment is applied separately on each row.
fn background_rects(
    layout: &TextLayout,
    align: TextAlign,
    range: &Range<usize>,
) -> Vec<Bounds<Pixels>> {
    let bounds = layout.bounds();
    let line_height = layout.line_height();
    let mut result = Vec::new();
    let mut line_start = 0;
    let mut y = bounds.top();
    for line in layout.line_layouts() {
        let unwrapped = &line.unwrapped_layout;
        let ends = line
            .wrap_boundaries
            .iter()
            .map(|boundary| unwrapped.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index)
            .chain([line.len()]);
        let mut row_start = 0;
        for row_end in ends {
            let from = range.start.saturating_sub(line_start).max(row_start);
            let to = range.end.saturating_sub(line_start).min(row_end);
            if from < to {
                let row_x = unwrapped.x_for_index(row_start);
                let row_width = unwrapped.x_for_index(row_end) - row_x;
                let remaining = bounds.size.width - row_width;
                let shift = match align {
                    TextAlign::Left => px(0.),
                    TextAlign::Center => remaining * 0.5,
                    TextAlign::Right => remaining,
                };
                result.push(Bounds::new(
                    point(
                        bounds.left() + shift + unwrapped.x_for_index(from) - row_x,
                        y,
                    ),
                    size(
                        unwrapped.x_for_index(to) - unwrapped.x_for_index(from),
                        line_height,
                    ),
                ));
            }
            row_start = row_end;
            y += line_height;
        }
        line_start += line.len() + 1;
    }
    result
}

fn highlights(
    text: &str,
    marks: Vec<(Range<usize>, HighlightStyle)>,
    selected: Option<Range<usize>>,
    theme: &Theme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some(selected) = selected else {
        return marks;
    };
    let mut edges: Vec<_> = marks
        .iter()
        .flat_map(|(range, _)| [range.start, range.end])
        .chain([selected.start, selected.end])
        .filter(|&edge| edge <= text.len() && text.is_char_boundary(edge))
        .collect();
    edges.sort_unstable();
    edges.dedup();
    let mut current = 0;
    edges
        .windows(2)
        .filter_map(|pair| {
            let range = pair[0]..pair[1];
            while marks
                .get(current)
                .is_some_and(|(mark, _)| mark.end <= range.start)
            {
                current += 1;
            }
            let style = marks
                .get(current)
                .filter(|(mark, _)| mark.start <= range.start && range.end <= mark.end);
            let in_selection = selected.start <= range.start && range.end <= selected.end;
            if style.is_none() && !in_selection {
                return None;
            }
            let mut style = style.map_or_else(HighlightStyle::default, |(_, style)| *style);
            if in_selection {
                let background = theme.primary_wash();
                let foreground = style
                    .color
                    .map_or(theme.foreground, |color| u32::from(color.to_rgb()) >> 8);
                style.background_color = Some(rgb(background).into());
                style.color = Some(rgb(super::text_ink(foreground, background)).into());
            }
            Some((range, style))
        })
        .collect()
}

#[cfg(test)]
mod tests;
