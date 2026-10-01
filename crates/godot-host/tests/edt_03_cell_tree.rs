// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use soe_esm::GlobalFormId;
use soe_godot_host::data_source::DataSource;

#[test]
fn edt_03_lists_cells_and_streams_selected_references() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the installed-data editor-cell check");
        return;
    };

    let source = DataSource::open(&data).unwrap_or_else(|error| panic!("open Data: {error}"));
    let cells = source.editor_cells();
    assert!(
        !cells.is_empty(),
        "the cell tree has no winning CELL records"
    );
    assert!(cells.iter().any(|cell| cell.interior));
    assert!(cells
        .iter()
        .any(|cell| !cell.interior && cell.world_id.is_some()));
    assert!(cells.windows(2).all(|pair| {
        (
            pair[0].interior,
            &pair[0].world_id,
            pair[0].y,
            pair[0].x,
            &pair[0].id,
        ) <= (
            pair[1].interior,
            &pair[1].world_id,
            pair[1].y,
            pair[1].x,
            &pair[1].id,
        )
    }));

    let desired = source
        .resolve_cell("WhiterunBanneredMare")
        .filter(|id| source.cell_index().references(*id).len() > 50)
        .or_else(|| source.first_populated_interior_cell(50))
        .expect("installed data has no populated interior cell");
    let row = cells
        .iter()
        .find(|cell| cell.id == desired.to_string())
        .expect("selected cell is missing from the editor tree");
    assert!(row.references > 50);

    let parsed_id = row.id.split_once(':').and_then(|(file, object)| {
        Some(GlobalFormId {
            file: file.parse().ok()?,
            object: u32::from_str_radix(object, 16).ok()?,
        })
    });
    assert_eq!(parsed_id, Some(desired));

    let loaded = source
        .stream_cell_id(desired)
        .unwrap_or_else(|error| panic!("selected cell failed to stream: {error}"));
    assert_eq!(
        row.references,
        source.cell_index().references(desired).len()
    );
    assert!(loaded.stats.references > 0);
    assert!(loaded.stats.references <= row.references);
    assert!(loaded.stats.meshes_parsed > 0);
    assert!(!loaded.surfaces.is_empty());
}
