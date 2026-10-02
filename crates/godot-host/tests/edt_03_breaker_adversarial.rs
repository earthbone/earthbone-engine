// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{collections::BTreeSet, path::PathBuf};

use soe_esm::{tag, GlobalFormId};
use soe_godot_host::data_source::DataSource;

fn installed_source() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|error| panic!("open Data: {error}")))
}

fn parse_id(value: &str) -> GlobalFormId {
    let (file, object) = value.split_once(':').expect("cell ID lacks separator");
    GlobalFormId {
        file: file.parse().expect("cell file index is not numeric"),
        object: u32::from_str_radix(object, 16).expect("cell object ID is not hex"),
    }
}

#[test]
fn edt_03_cell_tree_ids_round_trip_and_are_unique() {
    let Some(source) = installed_source() else {
        eprintln!("SKYRIM_DATA not set, skipping EDT-03 cell ID check");
        return;
    };
    let cells = source.editor_cells();
    let mut ids = BTreeSet::new();
    for cell in &cells {
        assert!(
            ids.insert(cell.id.as_str()),
            "duplicate cell ID {}",
            cell.id
        );
        let id = parse_id(&cell.id);
        assert_eq!(id.to_string(), cell.id, "cell ID is not canonical");
        assert_eq!(source.shared_order().winner(id).unwrap().tag, tag::CELL);
    }
    assert!(!cells.is_empty(), "editor tree has no winning cells");
}

#[test]
fn edt_03_empty_cell_is_a_valid_selection_target() {
    let Some(source) = installed_source() else {
        eprintln!("SKYRIM_DATA not set, skipping EDT-03 empty cell check");
        return;
    };
    let empty = source
        .editor_cells()
        .into_iter()
        .find(|cell| cell.references == 0)
        .expect("installed data has no empty cell");
    let id = parse_id(&empty.id);
    let loaded = source
        .stream_cell_id(id)
        .unwrap_or_else(|error| panic!("empty cell failed to stream: {error}"));
    assert_eq!(loaded.stats.references, 0);
}

#[test]
fn edt_03_duplicate_editor_names_keep_distinct_cell_identity() {
    let Some(source) = installed_source() else {
        eprintln!("SKYRIM_DATA not set, skipping EDT-03 duplicate name check");
        return;
    };
    let cells = source.editor_cells();
    let duplicate = cells.iter().enumerate().find_map(|(index, cell)| {
        cells[index + 1..]
            .iter()
            .find(|other| other.name == cell.name && other.id != cell.id)
            .map(|other| (cell, other))
    });
    let Some((first, second)) = duplicate else {
        eprintln!("installed load order has no duplicate cell editor names");
        return;
    };
    let first_id = parse_id(&first.id);
    let second_id = parse_id(&second.id);
    assert_ne!(first_id, second_id);
    let first_loaded = source.stream_cell_id(first_id).unwrap();
    let second_loaded = source.stream_cell_id(second_id).unwrap();
    assert!(first_loaded.stats.references <= first.references);
    assert!(second_loaded.stats.references <= second.references);
}

#[test]
fn edt_03_repeated_cell_streams_return_the_same_reference_counts() {
    let Some(source) = installed_source() else {
        eprintln!("SKYRIM_DATA not set, skipping EDT-03 repeat stream check");
        return;
    };
    let cell = source
        .editor_cells()
        .into_iter()
        .find(|cell| cell.interior && cell.references > 50)
        .expect("installed data has no populated interior cell");
    let id = parse_id(&cell.id);
    let first = source.stream_cell_id(id).unwrap();
    let second = source.stream_cell_id(id).unwrap();
    assert_eq!(first.stats.references, second.stats.references);
    assert_eq!(first.stats.meshes_parsed, second.stats.meshes_parsed);
    assert_eq!(first.stats.meshes_failed, second.stats.meshes_failed);
    assert_eq!(first.surfaces.len(), second.surfaces.len());
    assert_eq!(first.stats.references, cell.references);
}
