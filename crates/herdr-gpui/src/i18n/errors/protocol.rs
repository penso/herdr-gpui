//! `herdr_protocol::Error`: a frame or surface the daemon sent that does not
//! decode or validate, from `error-protocol.ftl`.

use super::Localize;
use crate::i18n::fl;
use herdr_client::protocol::Error;

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::CellCount => fl!("error-protocol-cell-count"),
            Self::ClipboardImageExtension => fl!("error-protocol-clipboard-image-extension"),
            Self::ClipboardImageSize => fl!("error-protocol-clipboard-image-size"),
            Self::ClipboardImageTarget => fl!("error-protocol-clipboard-image-target"),
            Self::CursorBounds => fl!("error-protocol-cursor-bounds"),
            Self::Decode(error) => fl!("error-protocol-decode", error = error.to_string()),
            Self::DeltaGridLimit => fl!("error-protocol-delta-grid-limit"),
            Self::DeltaMetadataCells => fl!("error-protocol-delta-metadata-cells"),
            Self::DeltaPopup => fl!("error-protocol-delta-popup"),
            Self::DeltaSpan => fl!("error-protocol-delta-span"),
            Self::EmptyFrame => fl!("error-protocol-empty-frame"),
            Self::Encode(error) => fl!("error-protocol-encode", error = error.to_string()),
            Self::EncodingJson(error) => {
                fl!("error-protocol-encoding-json", error = error.to_string())
            }
            Self::EncodingLimit => fl!("error-protocol-encoding-limit"),
            Self::EncodingPayload(error) => {
                fl!("error-protocol-encoding-payload", error = error.to_string())
            }
            Self::FrameLimit => fl!("error-protocol-frame-limit"),
            Self::HyperlinkIndex => fl!("error-protocol-hyperlink-index"),
            Self::Io(error) => fl!("error-protocol-io", error = error.to_string()),
            Self::PatchCursorBounds => fl!("error-protocol-patch-cursor-bounds"),
            Self::PatchGeometry => fl!("error-protocol-patch-geometry"),
            Self::PatchIdentity => fl!("error-protocol-patch-identity"),
            Self::PatchRowBounds => fl!("error-protocol-patch-row-bounds"),
            Self::ScrollBounds => fl!("error-protocol-scroll-bounds"),
            Self::ScrollCount => fl!("error-protocol-scroll-count"),
            Self::ScrollPayload => fl!("error-protocol-scroll-payload"),
            Self::ScrollTruncated => fl!("error-protocol-scroll-truncated"),
            Self::SurfaceBaseline => fl!("error-protocol-surface-baseline"),
            Self::TrailingBytes => fl!("error-protocol-trailing-bytes"),
            // keep-sorted end
        }
    }
}
