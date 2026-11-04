//! Lines a cell draws across its width: the SGR underline and strikethrough,
//! and the dashed underline that marks an explicit hyperlink at rest.
use crate::terminal::{STRIKETHROUGH, UNDERLINE};
use sheprd_client::protocol::{CellData, FrameData};

const DASH: f32 = 4.;
const GAP: f32 = 3.;

/// How far below a row's top its underline is drawn. A hovered link's solid
/// underline shares it, so it replaces the dashes instead of doubling them.
pub(super) fn underline_offset(cell_height: f32) -> f32 {
    cell_height - 2.
}

pub(super) fn decoration_offsets(
    cell: &CellData,
    cell_height: f32,
) -> impl Iterator<Item = f32> + '_ {
    [
        (UNDERLINE, underline_offset(cell_height)),
        (STRIKETHROUGH, cell_height / 2.),
    ]
    .into_iter()
    .filter_map(|(modifier, y)| (cell.modifier & modifier != 0).then_some(y))
}

/// Whether an explicit hyperlink covers the cell at `index`. A wide glyph's
/// continuation cell may carry no link of its own, so it takes its glyph's.
fn hyperlinked(frame: &FrameData, index: usize) -> bool {
    let row_start = index - index % usize::from(frame.width).max(1);
    (row_start..=index)
        .rev()
        .map_while(|index| frame.cells.get(index))
        .find(|cell| !cell.skip || cell.hyperlink.is_some())
        .is_some_and(|cell| cell.hyperlink.is_some())
}

/// The dashes of a dashed line between `left` and `right`, in pixels from the
/// grid's left edge. Their phase is the grid's, not the cell's, so the dashes
/// of neighboring cells form one even line.
fn dashes(left: f32, right: f32) -> impl Iterator<Item = (f32, f32)> {
    let period = DASH + GAP;
    let first = (left / period).floor();
    (0u16..)
        .map(move |n| {
            let start = (first + f32::from(n)) * period;
            (start.max(left), (start + DASH).min(right))
        })
        .take_while(move |(start, _)| *start < right)
        .filter(|(start, end)| start < end)
}

/// Every line the cell at `index` draws, as its offset below the row's top
/// and its horizontal extent in pixels from the grid's left edge. A linked
/// cell's underline is dashed whether or not the program underlined it too.
pub(crate) fn decoration_spans(
    frame: &FrameData,
    index: usize,
    cell_width: f32,
    cell_height: f32,
) -> impl Iterator<Item = (f32, f32, f32)> + '_ {
    let column = (index % usize::from(frame.width).max(1)) as f32;
    let (left, right) = (column * cell_width, (column + 1.) * cell_width);
    let linked = hyperlinked(frame, index);
    let underline = underline_offset(cell_height);
    let solid = frame
        .cells
        .get(index)
        .into_iter()
        .flat_map(move |cell| decoration_offsets(cell, cell_height))
        .filter(move |y| !(linked && *y == underline))
        .map(move |y| (y, left, right));
    let dashed = linked
        .then(|| dashes(left, right))
        .into_iter()
        .flatten()
        .map(move |(start, end)| (underline, start, end));
    solid.chain(dashed)
}
