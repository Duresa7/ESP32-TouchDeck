use serde::Deserialize;
use td_core::Layout;
use td_core::live::{PixelRect, ScreenSize, art_size, cell_rect, tile_rect};

#[derive(Deserialize)]
struct Size {
    w: u16,
    h: u16,
}

#[derive(Deserialize)]
struct Grid {
    rows: u8,
    cols: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Span {
    row: u8,
    col: u8,
    row_span: u8,
    col_span: u8,
}

#[derive(Deserialize)]
struct Rect {
    x: i32,
    y: i32,
    w: u16,
    h: u16,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    screen: Size,
    full_screen: bool,
    grid: Grid,
    tile: Span,
    rect: Rect,
}

#[derive(Deserialize)]
struct Vectors {
    cases: Vec<Case>,
}

#[test]
fn cell_rects_match_the_firmware_vectors() {
    let vectors: Vectors = serde_json::from_str(include_str!(
        "../../../../schema/live/tile-geometry-vectors.json"
    ))
    .unwrap();
    for c in vectors.cases {
        let got = cell_rect(
            ScreenSize {
                w: c.screen.w,
                h: c.screen.h,
            },
            c.full_screen,
            [c.grid.rows, c.grid.cols],
            [c.tile.row, c.tile.col, c.tile.row_span, c.tile.col_span],
        );
        let want = (c.rect.w > 0).then_some(PixelRect {
            x: c.rect.x,
            y: c.rect.y,
            w: c.rect.w,
            h: c.rect.h,
        });
        assert_eq!(got, want, "{}", c.name);
    }
}

#[test]
fn hostile_grids_and_screens_have_no_rect() {
    let screen = ScreenSize { w: 1024, h: 600 };
    assert_eq!(cell_rect(screen, false, [1, 1], [0, 0, 255, 255]), None);
    assert_eq!(cell_rect(screen, false, [255, 255], [0, 0, 1, 1]), None);
    assert_eq!(cell_rect(screen, false, [0, 1], [0, 0, 1, 1]), None);
    assert_eq!(cell_rect(screen, false, [2, 2], [1, 1, 0, 1]), None);
    assert_eq!(
        cell_rect(ScreenSize { w: 0, h: 600 }, false, [1, 1], [0, 0, 1, 1]),
        None
    );
    assert!(
        cell_rect(
            ScreenSize { w: u16::MAX, h: 3 },
            false,
            [8, 8],
            [7, 7, 1, 1]
        )
        .is_some()
    );
    assert!(
        cell_rect(
            ScreenSize {
                w: u16::MAX,
                h: u16::MAX
            },
            true,
            [8, 8],
            [0, 0, 8, 8]
        )
        .is_some()
    );
}

#[test]
fn a_layout_tile_takes_its_page_grid_and_span() {
    let layout = Layout::parse(include_bytes!("../../../../schema/examples/graph.json")).unwrap();
    let td_core::layout::Page::Grid {
        id,
        rows,
        cols,
        tiles,
        ..
    } = &layout.pages[0]
    else {
        panic!("graph example starts with a grid Page");
    };
    let tile = &tiles[0];
    let screen = ScreenSize { w: 1024, h: 600 };
    assert_eq!(
        tile_rect(&layout, id, &tile.id, screen),
        cell_rect(
            screen,
            layout.full_screen,
            [*rows, *cols],
            [tile.row, tile.col, tile.row_span, tile.col_span]
        )
    );
    assert!(tile_rect(&layout, id, &tile.id, screen).is_some());
    assert_eq!(tile_rect(&layout, id, "missing", screen), None);
    assert_eq!(tile_rect(&layout, "missing", &tile.id, screen), None);
}

#[test]
fn art_sizes_match_the_shared_now_playing_vectors() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../schema/live/now-playing-vectors.json"
    ))
    .unwrap();
    for c in vectors["cases"].as_array().unwrap() {
        let w = u16::try_from(c["w"].as_u64().unwrap()).unwrap();
        let h = u16::try_from(c["h"].as_u64().unwrap()).unwrap();
        let want = c["art"]["size"].as_u64().map(|s| u16::try_from(s).unwrap());
        assert_eq!(art_size(w, h), want, "{}", c["name"]);
    }
    assert_eq!(art_size(u16::MAX, u16::MAX), Some(256));
    assert_eq!(art_size(0, 0), None);
}
