#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde::Serialize;
use sheprd_protocol::*;

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!("fixtures/endpoint-snapshot-v1.json")).unwrap()
}

fn surface(snapshot: &ClientShellSnapshot) -> PaneSurfaceFrame {
    PaneSurfaceFrame {
        boot_id: snapshot.boot_id.clone(),
        projection_revision: snapshot.revision,
        surface_revision: 251,
        frame: FrameData {
            width: 0,
            height: 0,
            cells: vec![],
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![],
        splits: vec![],
        popup: None,
        graphics: SurfaceGraphicsScene::default(),
    }
}

fn payload(value: &impl Serialize) -> Vec<u8> {
    encode_message(value, MAX_FRAME_SIZE).unwrap()[4..].to_vec()
}

#[test]
fn actual_surface_variants_match_only_after_the_complete_identity_header() {
    let snapshot = snapshot();
    let baseline = surface(&snapshot);
    let mut next = baseline.clone();
    next.surface_revision += 1;
    let patch = PaneSurfacePatch {
        boot_id: baseline.boot_id.clone(),
        projection_revision: baseline.projection_revision,
        base_surface_revision: baseline.surface_revision,
        surface_revision: next.surface_revision,
        rows: vec![],
        panes: vec![],
        cursor: None,
    };
    for (message, header) in [
        (
            ServerMessage::PaneSurface(next.clone()),
            payload(&(
                13u32,
                &next.boot_id,
                next.projection_revision,
                next.surface_revision,
            )),
        ),
        (
            ServerMessage::PaneSurfacePatch(patch.clone()),
            payload(&(
                19u32,
                &patch.boot_id,
                patch.projection_revision,
                patch.base_surface_revision,
                patch.surface_revision,
            )),
        ),
    ] {
        let bytes = payload(&message);
        assert!(bytes.starts_with(&header));
        for end in 0..header.len() {
            assert!(!has_current_surface_prefix(
                &bytes[..end],
                &snapshot,
                &baseline
            ));
        }
        assert!(has_current_surface_prefix(&header, &snapshot, &baseline));
        assert!(has_current_surface_prefix(&bytes, &snapshot, &baseline));
        // Prefix inspection deliberately does not relax complete-frame decoding.
        assert!(decode_payload::<ServerMessage>(&header).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(matches!(
            decode_payload::<ServerMessage>(&trailing),
            Err(Error::TrailingBytes)
        ));
    }
}

#[test]
fn current_identity_and_patch_baseline_are_required() {
    let snapshot = snapshot();
    let baseline = surface(&snapshot);
    let probe =
        |tag, boot: &str, projection, base, next: Option<u64>, retained: &PaneSurfaceFrame| {
            let mut bytes = payload(&(tag, boot, projection, base));
            if let Some(next) = next {
                bytes.extend(payload(&next));
            }
            has_current_surface_prefix(&bytes, &snapshot, retained)
        };
    assert!(probe(
        13u32,
        &snapshot.boot_id,
        snapshot.revision,
        252u64,
        None,
        &baseline
    ));
    for (boot, projection, revision) in [
        ("other", snapshot.revision, 252),
        (&*snapshot.boot_id, snapshot.revision + 1, 252),
        (&*snapshot.boot_id, snapshot.revision, 251),
        (&*snapshot.boot_id, snapshot.revision, 250),
    ] {
        assert!(!probe(13, boot, projection, revision, None, &baseline));
    }
    assert!(probe(
        19,
        &snapshot.boot_id,
        snapshot.revision,
        251,
        Some(252),
        &baseline
    ));
    assert!(!probe(
        19,
        &snapshot.boot_id,
        snapshot.revision,
        250,
        Some(251),
        &baseline
    ));
    assert!(!probe(
        19,
        &snapshot.boot_id,
        snapshot.revision,
        251,
        Some(253),
        &baseline
    ));
    let mut old = baseline.clone();
    old.projection_revision -= 1;
    assert!(probe(
        13,
        &snapshot.boot_id,
        snapshot.revision,
        252,
        None,
        &old
    ));
    assert!(!probe(
        19,
        &snapshot.boot_id,
        snapshot.revision,
        251,
        Some(252),
        &old
    ));
    old.boot_id = "old".into();
    assert!(!probe(
        13,
        &snapshot.boot_id,
        snapshot.revision,
        252,
        None,
        &old
    ));
    let mut overflow = baseline.clone();
    overflow.surface_revision = u64::MAX;
    assert!(!probe(
        19,
        &snapshot.boot_id,
        snapshot.revision,
        u64::MAX,
        Some(0),
        &overflow
    ));
}

#[test]
fn malformed_or_unbounded_headers_never_admit_input() {
    let snapshot = snapshot();
    let baseline = surface(&snapshot);
    for tag in [0u32, 12, 14, 18, 20, 250, u32::MAX] {
        let bytes = payload(&(tag, &snapshot.boot_id, snapshot.revision, 252u64));
        assert!(!has_current_surface_prefix(&bytes, &snapshot, &baseline));
    }
    for bytes in [
        vec![255],                                             // Invalid varint discriminant.
        vec![253, 13, 0, 0, 0, 0, 0, 0, 0], // u64 marker cannot encode a u32 tag.
        vec![13, 253, 255, 255, 255, 255, 255, 255, 255, 255], // Huge string length.
        vec![13, 1, 255, 7, 252],           // Invalid UTF-8 boot.
    ] {
        assert!(!has_current_surface_prefix(&bytes, &snapshot, &baseline));
    }
    let mut long = snapshot.clone();
    long.boot_id = "b".repeat(513);
    let retained = surface(&long);
    let bytes = payload(&(13u32, &long.boot_id, long.revision, 252u64));
    assert!(!has_current_surface_prefix(&bytes, &long, &retained));
}

#[test]
fn standard_varint_widths_are_preserved_without_fixed_offset_assumptions() {
    for revision in [250, 251, u16::MAX as u64 + 1, u32::MAX as u64 + 1, u64::MAX] {
        let mut snapshot = snapshot();
        snapshot.boot_id = "b".repeat(251);
        snapshot.revision = revision;
        let baseline = surface(&snapshot);
        let bytes = payload(&(13u32, &snapshot.boot_id, revision, 252u64));
        assert!(has_current_surface_prefix(&bytes, &snapshot, &baseline));
        for end in 0..bytes.len() {
            assert!(!has_current_surface_prefix(
                &bytes[..end],
                &snapshot,
                &baseline
            ));
        }
    }
    let snapshot = snapshot();
    let mut baseline = surface(&snapshot);
    baseline.popup = Some(Box::new(ClientShellPopupSurface {
        terminal_id: "popup".into(),
        title: "QA".into(),
        width: None,
        height: None,
        frame: baseline.frame.clone(),
        mouse_reporting: false,
        sgr_pixel_mouse: false,
        pixel_width: 0,
        pixel_height: 0,
    }));
    let bytes = payload(&(13u32, &snapshot.boot_id, snapshot.revision, 252u64));
    assert!(!has_current_surface_prefix(&bytes, &snapshot, &baseline));
}
