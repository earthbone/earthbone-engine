// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use soe_godot_host::data_source::DataSource;
use soe_esm::tag;

#[test]
fn edt_03_cell_tree_covers_every_winning_cell_including_empty_cells() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed-data editor-cell audit");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|error| panic!("open Data: {error}"));
    let order = source.shared_order();
    let expected: BTreeSet<String> = order
        .winners()
        .filter_map(|(id, record_ref)| {
            (order.record(record_ref).tag == tag::CELL).then(|| id.to_string())
        })
        .collect();
    let cells = source.editor_cells();
    let actual: BTreeSet<String> = cells.iter().map(|cell| cell.id.clone()).collect();

    assert_eq!(actual, expected, "cell tree omitted or duplicated a winner");
    assert!(cells.iter().any(|cell| cell.references == 0));
    assert_eq!(cells, source.editor_cells(), "tree ordering changed between reads");
}

#[test]
fn edt_03_tree_form_ids_stream_the_selected_cell_by_identity() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed-data cell selection audit");
        return;
    };
    let source = DataSource::open(&data).unwrap_or_else(|error| panic!("open Data: {error}"));
    let desired = source
        .resolve_cell("WhiterunBanneredMare")
        .or_else(|| source.first_populated_interior_cell(50))
        .expect("no populated interior cell is available");
    let row = source
        .editor_cells()
        .into_iter()
        .find(|cell| cell.id == desired.to_string())
        .expect("resolved cell is missing from the tree");
    let (file, object) = row.id.split_once(':').expect("cell id has no separator");
    assert_eq!(file.parse::<u16>().unwrap(), desired.file);
    assert_eq!(u32::from_str_radix(object, 16).unwrap(), desired.object);

    let loaded = source
        .stream_cell_id(desired)
        .unwrap_or_else(|error| panic!("selected cell failed to stream: {error}"));
    assert!(loaded.stats.references > 0);
    assert!(loaded.stats.references <= row.references);
}
