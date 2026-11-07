//! Pictures and diagrams a chat draws: the agent's images and its Mermaid
//! fences, turned into textures off the UI thread. Both come from the
//! program and are untrusted: an image is decoded with the same pixel and
//! memory caps as a pane's graphics, and a diagram's source is bounded and
//! drawn by a renderer that loads nothing it names.
//!
//! Each visual is keyed by a hash of what it draws, so a message that keeps
//! streaming does not redraw a diagram it already finished. Diagrams follow
//! the theme: a theme change redraws them while the old texture stays up.

use super::rich::MathMode;
use crate::config::{FontConfig, Theme};
use base64::Engine as _;
use gpui::{RenderImage, SvgRenderer};
use image::{ImageFormat, ImageReader, Limits};
use sheprd_chat_protocol::{Image, ImageType};
mod remote;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    io::Cursor,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
    sync::Arc,
};

/// Bytes of Mermaid source drawn; a longer fence stays code.
pub(crate) const MAX_DIAGRAM_SOURCE: usize = 16 * 1024;
/// Decoded bytes one image may allocate.
const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;
/// Pixels on a side of a decoded image.
const MAX_IMAGE_SIDE: u32 = 8_192;
/// Textures are scaled to fit this side, which every GPUI atlas accepts.
const MAX_TEXTURE_SIDE: f32 = 4_096.;
/// Device pixels per diagram point, so labels stay sharp on dense screens.
const DIAGRAM_SCALE: f32 = 2.;
/// Visuals a window prepares at once, across all its chats; the rest wait
/// their turn.
pub(crate) const MAX_RUNNING: usize = 2;
/// Rich entries admitted per pane, including queued work and failures.
pub(crate) const MAX_RICH_VISUALS: usize = 128;
/// Retained formula textures per pane; images/diagrams keep their own limits.
pub(crate) const MAX_MATH_TEXTURE_BYTES: usize = 64 * 1024 * 1024;

pub(crate) type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    // keep-sorted start
    #[error("the image is not valid base64")]
    Base64(#[source] base64::DecodeError),
    #[error("the image could not be decoded")]
    Decode(#[source] image::ImageError),
    #[error("the diagram could not be read")]
    Diagram(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("the picture could not be fetched")]
    Fetch(#[source] Box<ureq::Error>),
    #[error("the formula could not be drawn: {0}")]
    Math(String),
    #[error("the formula exceeds the rendering limits")]
    MathLimit,
    #[error("the formula could not be parsed")]
    MathParse(#[source] ratex_parser::ParseError),
    #[error("the address does not answer with a picture")]
    NotAnImage,
    #[error("the diagram renderer failed")]
    Panicked,
    #[error("the diagram could not be drawn")]
    Raster(#[source] Box<dyn std::error::Error + Send + Sync>),
    // keep-sorted end
}

/// A visual's identity: a hash of what it draws.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VisualId(u64);

impl VisualId {
    pub(crate) fn of(content: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// What a visual is drawn from.
#[derive(Clone, Debug)]
pub(crate) enum Source {
    Image(Image),
    Code {
        language: String,
        source: Arc<str>,
    },
    Math {
        source: Arc<str>,
        mode: MathMode,
    },
    Diagram(Arc<str>),
    /// A picture on the web, asked for by the user.
    Remote(crate::browser::WebUrl),
}

impl Source {
    /// Code and math count against the rich budget; math also against the
    /// texture budget.
    fn kind(&self) -> Kind {
        match self {
            Self::Code { .. } => Kind::Code,
            Self::Math { .. } => Kind::Math,
            Self::Image(_) | Self::Diagram(_) | Self::Remote(_) => Kind::Other,
        }
    }
}

/// A texture and the size it is drawn at, in points.
#[derive(Clone)]
pub(crate) struct Drawn {
    pub(crate) texture: Arc<RenderImage>,
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// Distance from the top to the mathematical baseline, when this is math.
    pub(crate) baseline: Option<f32>,
}

/// Prepared by the same bounded worker queue; code has no GPU texture.
pub(crate) enum Output {
    Image(Drawn),
    Code(Vec<Vec<crate::review::highlight::Span>>),
}

impl Output {
    /// Bytes of its texture; code has none.
    fn texture_bytes(&self) -> usize {
        match self {
            Self::Image(drawn) => drawn
                .texture
                .as_bytes(0)
                .map_or(usize::MAX, |bytes| bytes.len()),
            Self::Code(_) => 0,
        }
    }

    fn texture(self) -> Option<Arc<RenderImage>> {
        match self {
            Self::Image(drawn) => Some(drawn.texture),
            Self::Code(_) => None,
        }
    }
}

/// What the view shows for a visual now.
pub(crate) enum Shown<'a> {
    Pending,
    Ready(&'a Drawn),
    Failed(&'a Error),
}

/// The theme a diagram is drawn in, as the colors and font it uses.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DiagramStyle {
    background: u32,
    border: u32,
    dark: bool,
    family: String,
    fill: u32,
    line: u32,
    note: u32,
    size: f32,
    text: u32,
}

impl DiagramStyle {
    pub(crate) fn of(theme: &Theme, font: &FontConfig) -> Self {
        Self {
            background: theme.background,
            border: theme.muted,
            dark: luminance(theme.background) < 0.5,
            family: font.family.clone(),
            fill: theme.surface,
            line: theme.subtext(),
            note: theme.active,
            size: font.size,
            text: theme.foreground,
        }
    }

    fn theme(&self) -> mermaid_rs_renderer::Theme {
        let hex = |color: u32| format!("#{:06x}", color & 0x00ff_ffff);
        let mut theme = if self.dark {
            mermaid_rs_renderer::Theme::dark()
        } else {
            mermaid_rs_renderer::Theme::modern()
        };
        // GPUI's own aliases such as `.SystemUIFont` mean nothing to the SVG
        // font database, so named sans faces follow the UI font.
        let family = self.family.replace(['"', '\'', '<', '>', '&'], "");
        let named = (!family.starts_with('.')).then(|| format!("'{family}', "));
        theme.font_family = format!(
            // i18n: data
            "{}'Helvetica Neue', 'Segoe UI', 'Noto Sans', 'IBM Plex Sans', Arial, sans-serif",
            named.unwrap_or_default()
        );
        theme.font_size = self.size.clamp(10., 24.);
        theme.background = hex(self.background);
        theme.primary_color = hex(self.fill);
        theme.primary_text_color = hex(self.text);
        theme.primary_border_color = hex(self.border);
        theme.line_color = hex(self.line);
        theme.secondary_color = hex(self.note);
        theme.tertiary_color = hex(self.fill);
        theme.edge_label_background = hex(self.background);
        theme.cluster_background = hex(self.background);
        theme.cluster_border = hex(self.border);
        theme.sequence_actor_fill = hex(self.fill);
        theme.sequence_actor_border = hex(self.border);
        theme.sequence_actor_line = hex(self.line);
        theme.sequence_note_fill = hex(self.note);
        theme.sequence_note_border = hex(self.border);
        theme.sequence_activation_fill = hex(self.note);
        theme.sequence_activation_border = hex(self.border);
        theme.text_color = hex(self.text);
        theme.pie_title_text_color = hex(self.text);
        theme.pie_section_text_color = hex(self.text);
        theme.pie_legend_text_color = hex(self.text);
        theme.pie_stroke_color = hex(self.background);
        theme.pie_outer_stroke_color = hex(self.border);
        theme
    }
}

fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| ((color >> shift) & 0xff) as f32 / 255.;
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// One visual to prepare in the background, and the ticket its result
/// must still match when it lands.
pub(crate) struct Job {
    pub(crate) id: VisualId,
    pub(crate) ticket: u64,
    work: Work,
}

enum Work {
    Image(Image),
    Code(String, Arc<str>),
    Math(Arc<str>, MathMode, Arc<DiagramStyle>),
    Diagram(Arc<str>, Arc<DiagramStyle>, SvgRenderer),
    Remote(crate::browser::WebUrl),
}

impl Job {
    /// Runs on the background executor.
    pub(crate) fn run(self) -> (VisualId, u64, Result<Output>) {
        let drawn = match self.work {
            Work::Image(image) => decode(&image).map(Output::Image),
            Work::Code(language, source) => {
                Ok(Output::Code(super::rich::highlight(&language, &source)))
            }
            Work::Math(source, mode, style) => catch_unwind(AssertUnwindSafe(|| {
                super::rich::draw_math(&source, mode, style.size, style.text)
            }))
            .unwrap_or(Err(Error::Panicked))
            .map(Output::Image),
            Work::Remote(url) => remote::fetch(&url).map(Output::Image),
            Work::Diagram(source, style, renderer) => {
                // The renderer is a third-party parser fed untrusted text; a
                // panic in it fails this diagram rather than the app.
                catch_unwind(AssertUnwindSafe(|| diagram(&source, &style, &renderer)))
                    .unwrap_or(Err(Error::Panicked))
                    .map(Output::Image)
            }
        };
        (self.id, self.ticket, drawn)
    }
}

/// Decodes an image into a texture, at its own pixel size in points.
pub(crate) fn decode(image: &Image) -> Result<Drawn> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&image.data)
        .map_err(Error::Base64)?;
    let format = match image.kind {
        ImageType::Gif => ImageFormat::Gif,
        ImageType::Jpeg => ImageFormat::Jpeg,
        ImageType::Png => ImageFormat::Png,
        ImageType::Webp => ImageFormat::WebP,
    };
    decode_bytes(bytes, format)
}

/// Decodes `bytes` of `format` within the pixel and memory caps.
fn decode_bytes(bytes: Vec<u8>, format: ImageFormat) -> Result<Drawn> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let pixels = reader.decode().map_err(Error::Decode)?.into_rgba8();
    let (width, height) = pixels.dimensions();
    let texture = crate::terminal_painter::texture(crate::terminal_painter::fit_bgra(pixels));
    Ok(Drawn {
        texture: Arc::new(texture),
        width: width as f32,
        height: height as f32,
        baseline: None,
    })
}

/// Lays out a Mermaid diagram and rasterizes its SVG at [`DIAGRAM_SCALE`].
fn diagram(source: &str, style: &DiagramStyle, renderer: &SvgRenderer) -> Result<Drawn> {
    use mermaid_rs_renderer as mermaid;
    // The lenient parser, as Mermaid itself accepts participants and nodes
    // a diagram never declared.
    let parsed = mermaid::parse_mermaid(source).map_err(|error| Error::Diagram(error.into()))?;
    let theme = style.theme();
    let config = mermaid::LayoutConfig::default();
    let layout = mermaid::compute_layout(&parsed.graph, &theme, &config);
    let size = mermaid::measure_svg_dimensions(&layout, &config, None);
    let (width, height) = (size.width.max(1.), size.height.max(1.));
    let svg = mermaid::render::render_svg_with_dimensions(
        &layout,
        &theme,
        &config,
        Some((width, height)),
    );
    let scale = DIAGRAM_SCALE.min(MAX_TEXTURE_SIDE / width.max(height));
    let texture = renderer
        .render_single_frame(svg.as_bytes(), scale)
        .map_err(|error| Error::Raster(Box::new(error)))?;
    Ok(Drawn {
        texture,
        width,
        height,
        baseline: None,
    })
}

enum Phase {
    Queued,
    Running(u64),
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Code,
    Math,
    Other,
}

struct Item {
    kind: Kind,
    phase: Phase,
    /// What is drawn now; a diagram keeps its old texture while it redraws.
    shown: Option<Result<Output>>,
    /// Kept for a diagram, which redraws with the theme; an image's bytes
    /// leave with its job.
    source: Option<Source>,
    /// Bytes of a valid formula the texture budget refused. It is drawn again
    /// once it is on screen and that much room exists, never every frame.
    refused_bytes: usize,
}

impl Item {
    /// Bytes of the texture drawn now.
    fn shown_bytes(&self) -> usize {
        self.shown
            .as_ref()
            .and_then(|shown| shown.as_ref().ok())
            .map_or(0, Output::texture_bytes)
    }
}

/// The visuals of one chat, by identity.
#[derive(Default)]
pub(crate) struct Visuals {
    /// Tells this pane's results from those of a pane recreated with the same
    /// target, whose tickets start over. Survives reconnects, unlike a pane.
    pub(crate) generation: Rc<()>,
    items: HashMap<VisualId, Item>,
    /// A preparation pass requests new visuals before it knows which old
    /// ones remain. Stage bounded overflow until retain frees those slots.
    deferred: HashMap<VisualId, Source>,
    math_bytes: usize,
    /// Textures no longer drawn, for the window to drop from the atlas.
    released: Vec<Arc<RenderImage>>,
    // Keep tickets even when a streamed revision forgets its old item: the
    // work is still in flight and must continue to occupy its worker slot.
    running: HashSet<u64>,
    rich_count: usize,
    style: Option<Arc<DiagramStyle>>,
    tickets: u64,
    /// What the lazy list drew last frame, which goes first and may push out
    /// history. Its sources share the prepared bodies' text.
    visible: RefCell<HashMap<VisualId, Source>>,
}

impl Visuals {
    /// Starts a frame's demand; the rows it draws then ask for their visuals.
    pub(crate) fn begin_frame(&self) {
        self.visible.borrow_mut().clear();
    }

    /// Notes that a row on screen draws `id`, a formula or code block. It is
    /// admitted with the next jobs, rebuilt if it was evicted.
    pub(crate) fn request_visible(&self, id: VisualId, source: Source) {
        let mut visible = self.visible.borrow_mut();
        if visible.len() < MAX_RICH_VISUALS {
            visible.entry(id).or_insert(source);
        }
    }

    /// Drops a visual to make room. One still running keeps its worker slot
    /// until it finishes, and its ticket cannot match a later admission.
    fn evict(&mut self, id: VisualId) {
        let Some(mut item) = self.items.remove(&id) else {
            return;
        };
        if item.kind != Kind::Other {
            self.rich_count -= 1;
        }
        if let Some(Ok(drawn)) = item.shown.take() {
            if item.kind == Kind::Math {
                self.math_bytes = self.math_bytes.saturating_sub(drawn.texture_bytes());
            }
            self.released.extend(drawn.texture());
        }
    }

    /// Admits what is on screen, evicting history when the budget is full,
    /// and queues refused formulas again where room now exists.
    fn admit_visible(&mut self, visible: &HashMap<VisualId, Source>) {
        for (id, source) in visible {
            if let Some(refused) = self.items.get(id).map(|item| item.refused_bytes) {
                if refused > 0 {
                    self.make_math_room(refused, visible);
                    if refused <= MAX_MATH_TEXTURE_BYTES.saturating_sub(self.math_bytes)
                        && let Some(item) = self.items.get_mut(id)
                    {
                        item.phase = Phase::Queued;
                        item.source = Some(source.clone());
                        item.refused_bytes = 0;
                    }
                }
                continue;
            }
            if self.rich_count >= MAX_RICH_VISUALS {
                // Finished work is cheaper to lose than a worker's result.
                let victim = self
                    .items
                    .iter()
                    .filter(|(id, item)| item.kind != Kind::Other && !visible.contains_key(*id))
                    .min_by_key(|(_, item)| matches!(item.phase, Phase::Running(_)))
                    .map(|(id, _)| *id);
                let Some(victim) = victim else {
                    break;
                };
                self.evict(victim);
            }
            self.want(*id, source.clone());
        }
    }

    /// Evicts drawn formulas off screen until `needed` bytes fit. A formula
    /// past the whole budget never fits, so it costs the others nothing.
    fn make_math_room(&mut self, needed: usize, visible: &HashMap<VisualId, Source>) {
        if needed > MAX_MATH_TEXTURE_BYTES {
            return;
        }
        while needed > MAX_MATH_TEXTURE_BYTES.saturating_sub(self.math_bytes) {
            let victim = self
                .items
                .iter()
                .find(|(id, item)| {
                    item.kind == Kind::Math
                        && !visible.contains_key(*id)
                        && matches!(item.phase, Phase::Done)
                        && item.shown.as_ref().is_some_and(Result::is_ok)
                })
                .map(|(id, _)| *id);
            let Some(victim) = victim else {
                break;
            };
            self.evict(victim);
        }
    }

    /// Asks for a visual; one already known is left as it is.
    pub(crate) fn want(&mut self, id: VisualId, source: Source) {
        if self.items.contains_key(&id) {
            return;
        }
        let kind = source.kind();
        if kind != Kind::Other {
            if self.rich_count >= MAX_RICH_VISUALS {
                if self.deferred.len() < MAX_RICH_VISUALS {
                    self.deferred.entry(id).or_insert(source);
                }
                return;
            }
            self.rich_count += 1;
        }
        self.items.entry(id).or_insert(Item {
            kind,
            phase: Phase::Queued,
            shown: None,
            source: Some(source),
            refused_bytes: 0,
        });
    }

    /// Forgets the visuals no entry draws any more.
    pub(crate) fn retain(&mut self, live: &HashSet<VisualId>) {
        self.visible.get_mut().retain(|id, _| live.contains(id));
        let released = &mut self.released;
        let math_bytes = &mut self.math_bytes;
        let rich_count = &mut self.rich_count;
        self.items.retain(|id, item| {
            let keep = live.contains(id);
            if !keep {
                if item.kind != Kind::Other {
                    *rich_count -= 1;
                }
                if let Some(Ok(drawn)) = item.shown.take() {
                    if item.kind == Kind::Math {
                        *math_bytes = math_bytes.saturating_sub(drawn.texture_bytes());
                    }
                    released.extend(drawn.texture());
                }
            }
            keep
        });
        // Only this pass's live requests are eligible. Anything still over
        // budget stays source; do not retain an unbounded retry backlog.
        for (id, source) in std::mem::take(&mut self.deferred) {
            if live.contains(&id) && self.rich_count < MAX_RICH_VISUALS {
                self.want(id, source);
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.retain(&HashSet::new());
    }

    /// Whether `id` was asked for, as a web picture is once the user asks.
    pub(crate) fn wants(&self, id: VisualId) -> bool {
        self.items.contains_key(&id)
    }

    pub(crate) fn shown(&self, id: VisualId) -> Shown<'_> {
        match self.items.get(&id).and_then(|item| item.shown.as_ref()) {
            Some(Ok(Output::Image(drawn))) => Shown::Ready(drawn),
            Some(Ok(Output::Code(_))) => Shown::Pending,
            Some(Err(error)) => Shown::Failed(error),
            None => Shown::Pending,
        }
    }

    pub(crate) fn code(&self, id: VisualId) -> Option<&[Vec<crate::review::highlight::Span>]> {
        match self.items.get(&id)?.shown.as_ref()?.as_ref().ok()? {
            Output::Code(spans) => Some(spans),
            Output::Image(_) => None,
        }
    }

    /// [`Self::jobs_within`] with the whole budget to itself.
    #[cfg(test)]
    pub(crate) fn jobs(&mut self, style: &DiagramStyle, renderer: &SvgRenderer) -> Vec<Job> {
        self.jobs_within(MAX_RUNNING, style, renderer)
    }

    /// The jobs to start now: queued visuals, what is on screen first, at
    /// most `capacity` of them and [`MAX_RUNNING`] running here at once, with
    /// every diagram queued again when the style changed.
    pub(crate) fn jobs_within(
        &mut self,
        capacity: usize,
        style: &DiagramStyle,
        renderer: &SvgRenderer,
    ) -> Vec<Job> {
        let visible = self.visible.take();
        self.admit_visible(&visible);
        let jobs = self.start_jobs(capacity, &visible, style, renderer);
        self.visible.replace(visible);
        jobs
    }

    fn start_jobs(
        &mut self,
        capacity: usize,
        visible: &HashMap<VisualId, Source>,
        style: &DiagramStyle,
        renderer: &SvgRenderer,
    ) -> Vec<Job> {
        if self.style.as_deref() != Some(style) {
            self.style = Some(Arc::new(style.clone()));
            for item in self.items.values_mut() {
                if matches!(item.source, Some(Source::Diagram(_) | Source::Math { .. })) {
                    item.phase = Phase::Queued;
                }
            }
        }
        let Some(style) = self.style.clone() else {
            return Vec::new();
        };
        let capacity = capacity.min(MAX_RUNNING.saturating_sub(self.running.len()));
        let mut jobs = Vec::new();
        for on_screen in [true, false] {
            for (id, item) in &mut self.items {
                if jobs.len() >= capacity {
                    return jobs;
                }
                if visible.contains_key(id) != on_screen || !matches!(item.phase, Phase::Queued) {
                    continue;
                }
                let work = match item.source.take() {
                    Some(Source::Diagram(source)) => {
                        let work = Work::Diagram(source.clone(), style.clone(), renderer.clone());
                        item.source = Some(Source::Diagram(source));
                        work
                    }
                    Some(Source::Math { source, mode }) => {
                        let work = Work::Math(source.clone(), mode, style.clone());
                        item.source = Some(Source::Math { source, mode });
                        work
                    }
                    Some(Source::Code { language, source }) => Work::Code(language, source),
                    Some(Source::Image(image)) => Work::Image(image),
                    Some(Source::Remote(url)) => Work::Remote(url),
                    None => continue,
                };
                self.tickets += 1;
                item.phase = Phase::Running(self.tickets);
                self.running.insert(self.tickets);
                jobs.push(Job {
                    id: *id,
                    ticket: self.tickets,
                    work,
                });
            }
        }
        jobs
    }

    /// Takes a job's result; one overtaken by a newer job or a forgotten
    /// visual is dropped. Whether the view changed.
    pub(crate) fn finish(&mut self, id: VisualId, ticket: u64, mut drawn: Result<Output>) -> bool {
        self.running.remove(&ticket);
        let Some(item) = self.items.get(&id) else {
            if let Ok(drawn) = drawn {
                self.released.extend(drawn.texture());
            }
            return false;
        };
        if !matches!(item.phase, Phase::Running(running) if running == ticket) {
            if let Ok(drawn) = drawn {
                self.released.extend(drawn.texture());
            }
            return false;
        }
        if item.kind == Kind::Math && self.visible.borrow().contains_key(&id) {
            let new_bytes = drawn.as_ref().map_or(0, Output::texture_bytes);
            let growth = new_bytes.saturating_sub(item.shown_bytes());
            let visible = self.visible.take();
            self.make_math_room(growth, &visible);
            self.visible.replace(visible);
        }
        let Some(item) = self.items.get_mut(&id) else {
            return false;
        };
        if item.kind == Kind::Math {
            self.math_bytes = self.math_bytes.saturating_sub(item.shown_bytes());
            let new_bytes = drawn.as_ref().map_or(0, Output::texture_bytes);
            if new_bytes > MAX_MATH_TEXTURE_BYTES.saturating_sub(self.math_bytes) {
                if let Ok(output) = drawn {
                    self.released.extend(output.texture());
                }
                drawn = Err(Error::MathLimit);
                // A cached failure stays source; a theme change must not
                // restart work that cannot fit and starve other queued jobs.
                item.source = None;
                item.refused_bytes = new_bytes;
            } else {
                self.math_bytes += new_bytes;
                item.refused_bytes = 0;
            }
        }
        item.phase = Phase::Done;
        if let Some(Ok(old)) = item.shown.replace(drawn) {
            self.released.extend(old.texture());
        }
        true
    }

    /// Textures to drop from the atlas.
    pub(crate) fn take_released(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.released)
    }

    #[cfg(test)]
    pub(crate) fn rich_budget(&self) -> (usize, usize) {
        (self.rich_count, self.math_bytes)
    }
}
