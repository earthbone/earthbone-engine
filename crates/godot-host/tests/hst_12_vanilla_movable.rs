// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-12 installed-data survey of authored movable REFR collision.

use soe_godot_host::data_source::DataSource;

#[test]
fn hst_12_vanilla_cell_has_authored_movable_reference() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).unwrap();
    let cell = source.resolve_cell("WhiterunBanneredMare").unwrap();
    let (placed, _) = source.placed_cell_colliders(cell).unwrap();
    let movable: Vec<_> = placed.iter().filter(|p| !p.collider.is_static()).collect();
    assert!(
        !movable.is_empty(),
        "Bannered Mare has no movable collision"
    );
    eprintln!("HST-12 movable colliders: {}", movable.len());
    for placed in movable.iter().take(5) {
        eprintln!("{:?} {:?}", placed.reference, placed.original.translation);
    }
}
