//! The host header pinned above the spaces list while that host's workspaces
//! scroll under it, as section headers stay in native lists, so a scrolled
//! list still says which machine its rows belong to.

/// A host header row as the spaces list last laid it out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HostHeader {
    /// The host's endpoint index.
    pub(super) host: usize,
    /// The row's top, relative to the top of the visible list: negative once
    /// it has scrolled above it.
    pub(super) top: f32,
    pub(super) height: f32,
}

/// The header to pin, with `top` where its copy is drawn: at the list's top,
/// or above it once pushed. The last header scrolled above the top pins in
/// place until the next one reaches the pinned copy's bottom edge and pushes
/// it up and out. `headers` follow list order. None while every header is at
/// or below the top, as the rows then show their own.
pub(super) fn pinned(headers: &[HostHeader]) -> Option<HostHeader> {
    let position = headers.iter().rposition(|header| header.top < 0.)?;
    let header = headers.get(position)?;
    let push = headers
        .get(position + 1)
        .map_or(0., |next| (next.top - header.height).min(0.));
    Some(HostHeader {
        top: push,
        ..*header
    })
}

/// Clamps `scroll` to the extent its last frame measured, which is what draws.
/// The wheel moves an offset without bounds and GPUI clamps it only in
/// prepaint, after render pinned headers and judged visibility from it: a list
/// that fits would pin a header it never scrolls past. Call before reading it.
pub(super) fn settle(scroll: &gpui::ScrollHandle) {
    let offset = scroll.offset();
    let clamped = offset
        .min(&gpui::Point::default())
        .max(&-scroll.max_offset());
    if clamped != offset {
        scroll.set_offset(clamped);
    }
}

/// How far down the list's top the pinned copy hides rows; 0 with none.
pub(super) fn cover(headers: &[HostHeader]) -> f32 {
    pinned(headers).map_or(0., |pinned| pinned.top + pinned.height)
}
