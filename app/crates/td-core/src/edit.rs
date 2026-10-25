//! Pure edits on a layout. Each takes a valid layout and returns the edited copy, or the
//! reason it refuses. A refused edit leaves the caller's layout as it was, so an editor
//! that only stores what these return never holds an invalid layout.
//! `app/src/lib/editor/validate.ts` mirrors every function here.

use std::fmt;

use crate::layout::{Action, Layout, Page, Tile};

pub const PAGES_MAX: usize = 8;
pub const TILES_MAX: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Overlaps(String),
    OutOfGrid { rows: u8, cols: u8 },
    TooManyTiles,
    TooManyPages,
    LastPage,
    NoPage(String),
    NoTile(String),
    NotGrid(String),
    Invalid { key: &'static str, message: String },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::Overlaps(id) => write!(f, "overlaps {id}"),
            Refusal::OutOfGrid { rows, cols } => write!(f, "outside the {rows}x{cols} grid"),
            Refusal::TooManyTiles => write!(f, "a page holds at most {TILES_MAX} tiles"),
            Refusal::TooManyPages => write!(f, "a layout holds at most {PAGES_MAX} pages"),
            Refusal::LastPage => f.write_str("a layout keeps at least 1 page"),
            Refusal::NoPage(id) => write!(f, "no page {id}"),
            Refusal::NoTile(id) => write!(f, "no tile {id}"),
            Refusal::NotGrid(id) => write!(f, "page {id} is a dashboard"),
            Refusal::Invalid { key, message } => write!(f, "{key}: {message}"),
        }
    }
}

type Edit = Result<Layout, Refusal>;

struct Grid<'a> {
    rows: u8,
    cols: u8,
    tiles: &'a mut Vec<Tile>,
}

fn grid<'a>(layout: &'a mut Layout, page: &str) -> Result<Grid<'a>, Refusal> {
    match layout.pages.iter_mut().find(|p| p.id() == page) {
        Some(Page::Grid {
            rows, cols, tiles, ..
        }) => Ok(Grid {
            rows: *rows,
            cols: *cols,
            tiles,
        }),
        Some(Page::Dashboard { .. }) => Err(Refusal::NotGrid(page.to_owned())),
        None => Err(Refusal::NoPage(page.to_owned())),
    }
}

fn tile_index(tiles: &[Tile], id: &str) -> Result<usize, Refusal> {
    tiles
        .iter()
        .position(|tile| tile.id == id)
        .ok_or_else(|| Refusal::NoTile(id.to_owned()))
}

fn covers(tile: &Tile, row: u8, col: u8) -> bool {
    (tile.row..tile.row + tile.row_span).contains(&row)
        && (tile.col..tile.col + tile.col_span).contains(&col)
}

/// Checks that `tile` fits the grid and overlaps no other tile, naming the first one it hits.
fn place(grid: &Grid, tile: &Tile, skip: Option<usize>) -> Result<(), Refusal> {
    if u16::from(tile.row) + u16::from(tile.row_span) > u16::from(grid.rows)
        || u16::from(tile.col) + u16::from(tile.col_span) > u16::from(grid.cols)
    {
        return Err(Refusal::OutOfGrid {
            rows: grid.rows,
            cols: grid.cols,
        });
    }
    let hit = grid.tiles.iter().enumerate().find(|(index, other)| {
        Some(*index) != skip
            && (tile.row..tile.row + tile.row_span)
                .any(|row| (tile.col..tile.col + tile.col_span).any(|col| covers(other, row, col)))
    });
    match hit {
        Some((_, other)) => Err(Refusal::Overlaps(other.id.clone())),
        None => Ok(()),
    }
}

/// Runs the full schema rules on an edited layout, so no edit can produce an invalid one.
fn checked(layout: Layout) -> Edit {
    match Layout::parse(&layout.to_canonical_json()) {
        Ok(_) => Ok(layout),
        Err(errors) => Err(Refusal::Invalid {
            key: errors[0].rule.key(),
            message: errors[0].message.clone(),
        }),
    }
}

pub fn add_tile(layout: &Layout, page: &str, tile: Tile) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    if grid.tiles.len() >= TILES_MAX {
        return Err(Refusal::TooManyTiles);
    }
    place(&grid, &tile, None)?;
    grid.tiles.push(tile);
    checked(next)
}

pub fn move_tile(layout: &Layout, page: &str, tile: &str, row: u8, col: u8) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    let index = tile_index(grid.tiles, tile)?;
    let moved = Tile {
        row,
        col,
        ..grid.tiles[index].clone()
    };
    place(&grid, &moved, Some(index))?;
    grid.tiles[index] = moved;
    checked(next)
}

pub fn resize_tile(layout: &Layout, page: &str, tile: &str, row_span: u8, col_span: u8) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    let index = tile_index(grid.tiles, tile)?;
    let resized = Tile {
        row_span,
        col_span,
        ..grid.tiles[index].clone()
    };
    if row_span == 0 || col_span == 0 {
        return Err(Refusal::OutOfGrid {
            rows: grid.rows,
            cols: grid.cols,
        });
    }
    place(&grid, &resized, Some(index))?;
    grid.tiles[index] = resized;
    checked(next)
}

pub fn set_action(layout: &Layout, page: &str, tile: &str, action: Action) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    let index = tile_index(grid.tiles, tile)?;
    grid.tiles[index].action = action;
    checked(next)
}

pub fn set_label(layout: &Layout, page: &str, tile: &str, label: &str) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    let index = tile_index(grid.tiles, tile)?;
    label.clone_into(&mut grid.tiles[index].label);
    checked(next)
}

pub fn set_icon(layout: &Layout, page: &str, tile: &str, icon: Option<String>) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    let index = tile_index(grid.tiles, tile)?;
    grid.tiles[index].icon = icon;
    checked(next)
}

pub fn remove_tile(layout: &Layout, page: &str, tile: &str) -> Edit {
    let mut next = layout.clone();
    let grid = grid(&mut next, page)?;
    let index = tile_index(grid.tiles, tile)?;
    grid.tiles.remove(index);
    checked(next)
}

pub fn add_page(layout: &Layout, page: Page) -> Edit {
    if layout.pages.len() >= PAGES_MAX {
        return Err(Refusal::TooManyPages);
    }
    let mut next = layout.clone();
    next.pages.push(page);
    checked(next)
}

/// Removes a page and turns every `page` action that jumped to it into `none`.
pub fn remove_page(layout: &Layout, page: &str) -> Edit {
    let index = layout
        .pages
        .iter()
        .position(|p| p.id() == page)
        .ok_or_else(|| Refusal::NoPage(page.to_owned()))?;
    if layout.pages.len() == 1 {
        return Err(Refusal::LastPage);
    }
    let mut next = layout.clone();
    next.pages.remove(index);
    for other in &mut next.pages {
        if let Page::Grid { tiles, .. } = other {
            for tile in tiles {
                if matches!(&tile.action, Action::Page { page: target } if target == page) {
                    tile.action = Action::None;
                }
            }
        }
    }
    checked(next)
}

/// The first free cell of a grid page in row order, for a new 1 by 1 tile.
pub fn free_cell(layout: &Layout, page: &str) -> Option<(u8, u8)> {
    let Some(Page::Grid {
        rows, cols, tiles, ..
    }) = layout.pages.iter().find(|p| p.id() == page)
    else {
        return None;
    };
    (0..*rows)
        .flat_map(|row| (0..*cols).map(move |col| (row, col)))
        .find(|&(row, col)| !tiles.iter().any(|tile| covers(tile, row, col)))
}

/// The first id of the form `<prefix><n>` that no page or tile of the layout uses.
pub fn fresh_id(layout: &Layout, prefix: &str) -> String {
    let taken = |id: &str| {
        layout.pages.iter().any(|page| {
            page.id() == id
                || matches!(page, Page::Grid { tiles, .. } if tiles.iter().any(|tile| tile.id == id))
        })
    };
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken(id))
        .expect("an unbounded range finds a free id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Modifier, Orientation};

    fn dev() -> Layout {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../schema/examples/dev.json"),
        )
        .unwrap();
        Layout::parse(&bytes).unwrap()
    }

    fn tile(id: &str, row: u8, col: u8) -> Tile {
        Tile {
            id: id.into(),
            label: id.to_uppercase(),
            row,
            col,
            row_span: 1,
            col_span: 1,
            icon: None,
            action: Action::None,
        }
    }

    fn grid_page(id: &str, rows: u8, cols: u8, tiles: Vec<Tile>) -> Page {
        Page::Grid {
            id: id.into(),
            title: id.to_uppercase(),
            rows,
            cols,
            tiles,
        }
    }

    fn small() -> Layout {
        Layout {
            schema: 1,
            name: "Small".into(),
            orientation: Orientation::Landscape,
            pages: vec![
                grid_page("main", 2, 3, vec![tile("a", 0, 0), tile("b", 0, 1)]),
                grid_page("other", 1, 1, vec![]),
            ],
        }
    }

    fn tiles<'a>(layout: &'a Layout, page: &str) -> &'a [Tile] {
        match layout.pages.iter().find(|p| p.id() == page) {
            Some(Page::Grid { tiles, .. }) => tiles,
            _ => panic!("no grid page {page}"),
        }
    }

    #[test]
    fn add_tile_accepts_a_free_cell_and_refuses_an_overlap_or_the_edge() {
        let layout = small();
        let added = add_tile(&layout, "main", tile("c", 1, 2)).unwrap();
        assert_eq!(tiles(&added, "main").len(), 3);
        assert_eq!(
            add_tile(&layout, "main", tile("c", 0, 1)),
            Err(Refusal::Overlaps("b".into()))
        );
        assert_eq!(
            add_tile(&layout, "main", tile("c", 2, 0)),
            Err(Refusal::OutOfGrid { rows: 2, cols: 3 })
        );
        assert_eq!(
            add_tile(&layout, "main", tile("a", 1, 0))
                .unwrap_err()
                .to_string()
                .split(':')
                .next(),
            Some("id.duplicate")
        );
        assert_eq!(
            add_tile(&layout, "nope", tile("c", 0, 0)),
            Err(Refusal::NoPage("nope".into()))
        );
    }

    #[test]
    fn a_65th_tile_is_refused() {
        let full: Vec<Tile> = (0..64u8)
            .map(|i| tile(&format!("t{i}"), i / 8, i % 8))
            .collect();
        let mut layout = small();
        layout.pages[1] = grid_page("other", 8, 8, full);
        assert!(Layout::parse(&layout.to_canonical_json()).is_ok());
        assert_eq!(
            add_tile(&layout, "other", tile("t64", 0, 0)),
            Err(Refusal::TooManyTiles)
        );
        assert_eq!(
            Refusal::TooManyTiles.to_string(),
            "a page holds at most 64 tiles"
        );
    }

    #[test]
    fn move_tile_lands_on_a_free_cell_and_names_the_tile_it_would_cover() {
        let layout = small();
        let moved = move_tile(&layout, "main", "a", 1, 2).unwrap();
        assert_eq!(
            (tiles(&moved, "main")[0].row, tiles(&moved, "main")[0].col),
            (1, 2)
        );
        let refused = move_tile(&layout, "main", "a", 0, 1).unwrap_err();
        assert_eq!(refused.to_string(), "overlaps b");
        assert_eq!(
            move_tile(&layout, "main", "a", 0, 3),
            Err(Refusal::OutOfGrid { rows: 2, cols: 3 })
        );
        assert_eq!(
            move_tile(&layout, "main", "a", 0, 0).unwrap(),
            layout,
            "a tile never overlaps itself"
        );
    }

    #[test]
    fn resize_tile_grows_into_free_cells_and_stops_at_the_edge_or_a_neighbour() {
        let layout = move_tile(&small(), "main", "b", 0, 2).unwrap();
        let grown = resize_tile(&layout, "main", "a", 2, 2).unwrap();
        assert_eq!(
            (
                tiles(&grown, "main")[0].row_span,
                tiles(&grown, "main")[0].col_span
            ),
            (2, 2)
        );
        assert_eq!(
            resize_tile(&layout, "main", "a", 3, 1),
            Err(Refusal::OutOfGrid { rows: 2, cols: 3 })
        );
        assert_eq!(
            resize_tile(&layout, "main", "a", 1, 3),
            Err(Refusal::Overlaps("b".into()))
        );
        assert!(resize_tile(&layout, "main", "a", 0, 1).is_err());
    }

    #[test]
    fn set_action_keeps_a_valid_action_and_refuses_an_invalid_one() {
        let layout = small();
        let key = Action::Key {
            usage: 62,
            mods: vec![Modifier::Ctrl, Modifier::Shift],
        };
        let set = set_action(&layout, "main", "a", key.clone()).unwrap();
        assert_eq!(tiles(&set, "main")[0].action, key);
        let refused = set_action(
            &layout,
            "main",
            "a",
            Action::Key {
                usage: 0,
                mods: vec![],
            },
        );
        assert!(
            matches!(
                refused,
                Err(Refusal::Invalid {
                    key: "key.usage-range",
                    ..
                })
            ),
            "{refused:?}"
        );
        let missing = set_action(
            &layout,
            "main",
            "a",
            Action::Page {
                page: "gone".into(),
            },
        );
        assert!(matches!(
            missing,
            Err(Refusal::Invalid {
                key: "action.page-missing",
                ..
            })
        ));
        assert_eq!(
            set_action(&layout, "main", "zz", Action::None),
            Err(Refusal::NoTile("zz".into()))
        );
    }

    #[test]
    fn labels_and_icons_follow_the_schema() {
        let layout = small();
        assert_eq!(
            tiles(&set_label(&layout, "main", "a", "Build").unwrap(), "main")[0].label,
            "Build"
        );
        assert!(matches!(
            set_label(&layout, "main", "a", " "),
            Err(Refusal::Invalid {
                key: "label.blank",
                ..
            })
        ));
        assert!(matches!(
            set_label(&layout, "main", "a", "seventeen letters"),
            Err(Refusal::Invalid {
                key: "label.too-long",
                ..
            })
        ));
        assert!(set_icon(&layout, "main", "a", Some("lv:mute".into())).is_ok());
        assert!(matches!(
            set_icon(&layout, "main", "a", Some("lv:nope".into())),
            Err(Refusal::Invalid {
                key: "icon.format",
                ..
            })
        ));
    }

    #[test]
    fn remove_tile_drops_only_that_tile() {
        let removed = remove_tile(&small(), "main", "a").unwrap();
        assert_eq!(
            tiles(&removed, "main")
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
        assert_eq!(
            remove_tile(&small(), "main", "a2"),
            Err(Refusal::NoTile("a2".into()))
        );
    }

    #[test]
    fn add_page_accepts_up_to_eight_and_refuses_a_ninth() {
        let mut layout = small();
        for n in 3..=8 {
            layout = add_page(&layout, grid_page(&format!("p{n}"), 4, 4, vec![])).unwrap();
        }
        assert_eq!(layout.pages.len(), 8);
        assert_eq!(
            add_page(&layout, grid_page("p9", 4, 4, vec![])),
            Err(Refusal::TooManyPages)
        );
        assert!(matches!(
            add_page(&small(), grid_page("main", 4, 4, vec![])),
            Err(Refusal::Invalid {
                key: "id.duplicate",
                ..
            })
        ));
    }

    #[test]
    fn a_removed_page_clears_the_page_actions_that_pointed_to_it() {
        let layout = set_action(
            &small(),
            "main",
            "a",
            Action::Page {
                page: "other".into(),
            },
        )
        .unwrap();
        let removed = remove_page(&layout, "other").unwrap();
        assert_eq!(removed.pages.len(), 1);
        assert_eq!(tiles(&removed, "main")[0].action, Action::None);
        assert_eq!(remove_page(&removed, "main"), Err(Refusal::LastPage));
        assert_eq!(
            remove_page(&layout, "gone"),
            Err(Refusal::NoPage("gone".into()))
        );
    }

    #[test]
    fn dev_json_edits_keep_its_frozen_ids() {
        let layout = dev();
        let page = layout.pages[0].id().to_owned();
        let (row, col) = free_cell(&layout, &page).expect("dev.json's first page has a free cell");
        let id = fresh_id(&layout, "tile");
        let added = add_tile(&layout, &page, tile(&id, row, col)).unwrap();
        assert_eq!(fresh_id(&added, "tile"), "tile2");
        assert!(
            add_tile(&added, &page, tile("other", row, col))
                .unwrap_err()
                .to_string()
                .starts_with("overlaps ")
        );
        let dashboard = layout
            .pages
            .iter()
            .find(|p| matches!(p, Page::Dashboard { .. }))
            .unwrap()
            .id()
            .to_owned();
        assert_eq!(
            add_tile(&layout, &dashboard, tile("x", 0, 0)),
            Err(Refusal::NotGrid(dashboard.clone()))
        );
        let removed = remove_page(&layout, &dashboard).unwrap();
        assert!(removed.pages.iter().all(|p| p.id() != dashboard));
    }
}
