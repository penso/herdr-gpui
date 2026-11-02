use crate::core_terminals::screen::Screen;
use herdr_backend::{CursorState, Exit, FrameData, PaneSurfacePatchRow, SurfaceUpdate};
use herdr_client::protocol::CellData;

fn cell(symbol: &str) -> CellData {
    CellData {
        symbol: symbol.into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    }
}

fn frame(width: u16, height: u16) -> FrameData {
    FrameData {
        cells: vec![cell(" "); usize::from(width) * usize::from(height)],
        width,
        height,
        cursor: None,
        hyperlinks: Vec::new(),
        graphics: Vec::new(),
    }
}

fn text(screen: &Screen) -> String {
    screen
        .frame
        .as_ref()
        .unwrap()
        .cells
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect()
}

#[test]
fn rows_patch_the_frame_in_place_and_stay_inside_it() {
    let mut screen = Screen::default();
    // Rows before any frame have nothing to patch.
    screen.apply(SurfaceUpdate::Rows {
        cursor: None,
        rows: vec![PaneSurfacePatchRow {
            x: 0,
            y: 0,
            cells: vec![cell("x")],
        }],
    });
    assert!(screen.frame.is_none());
    screen.apply(SurfaceUpdate::Frame(frame(3, 2)));
    let painted = screen.frame.clone();
    let cursor = CursorState {
        x: 2,
        y: 1,
        visible: true,
        shape: 0,
    };
    screen.apply(SurfaceUpdate::Rows {
        cursor: Some(cursor.clone()),
        rows: vec![
            PaneSurfacePatchRow {
                x: 1,
                y: 1,
                cells: vec![cell("a"), cell("b"), cell("overflow")],
            },
            PaneSurfacePatchRow {
                x: 0,
                y: 9,
                cells: vec![cell("off")],
            },
        ],
    });
    assert_eq!(text(&screen), "    ab");
    assert_eq!(screen.frame.as_ref().unwrap().cursor, Some(cursor));
    // A frame the window still paints is never changed under it.
    assert_eq!(painted.unwrap().cells[4].symbol, " ");
}

#[test]
fn titles_bells_and_endings_are_kept() {
    let mut screen = Screen::default();
    screen.apply(SurfaceUpdate::Title("vim".into()));
    screen.apply(SurfaceUpdate::Bell);
    screen.apply(SurfaceUpdate::Bell);
    screen.apply(SurfaceUpdate::Ended(Exit { code: Some(1) }));
    assert_eq!(
        (screen.title.as_deref(), screen.bells, screen.ended),
        (Some("vim"), 2, Some(Exit { code: Some(1) }))
    );
    screen.apply(SurfaceUpdate::Title(String::new()));
    assert_eq!(screen.title, None);
}
