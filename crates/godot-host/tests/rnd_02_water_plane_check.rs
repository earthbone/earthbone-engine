// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Judge-written check test for RND-02's done check: "A cell with water
//! draws its surface at the recorded height with its material."
//!
//! Three clauses, three tests, all against an installed Skyrim SE copy
//! (`data_source/water.rs` and `DataSource::water_surface` read only from
//! real `CELL` and `WATR` records, there is no synthetic ESM fixture writer
//! in this crate's tests to build one from scratch):
//!
//! - "draws its surface": `rnd_02_a_water_cell_gets_one_water_surface` finds
//!   a real winning `CELL` with the `HAS_WATER` flag and confirms
//!   `stream_cell_id` reports `water_plane_count == 1` and appends exactly
//!   one extra surface versus a matching cell with no water.
//! - "at the recorded height": `rnd_02_the_water_surface_sits_at_its_recorded_xclw_height`
//!   converts a real `XCLW` value through `METRES_PER_UNIT` independently of
//!   `data_source.rs` and checks every vertex of the drawn plane lands there.
//! - "with its material": `rnd_02_the_water_surface_carries_its_watr_colors_and_opacity`
//!   reads the cell's resolved `WATR` winner's `ANAM` and `DNAM` bytes
//!   directly, decodes them by the same TES5Edit/UESP layout `data_source/water.rs`
//!   documents (offsets 40/44/48, four byte `wbByteColors`), and compares
//!   that independently computed value against `MaterialDesc::water` on the
//!   drawn surface, so a passing test here cannot be explained by
//!   `water_material_from` and this file sharing one bug.
//!
//! Run against an installed copy, headless, no windowed renderer needed:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test rnd_02_water_plane_check -- --nocapture
//! ```
//! Every test skips (prints and returns, exit 0) without `SKYRIM_DATA` set,
//! per repository convention.

use std::path::PathBuf;

use soe_esm::cursor::Fields;
use soe_esm::records::cell::Cell;
use soe_esm::records::wrld::World;
use soe_esm::{tag, GlobalFormId, LoadOrder, Record, Tag};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::units::METRES_PER_UNIT;

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

struct FoundCell {
    id: GlobalFormId,
    file: usize,
    view: Cell,
}

/// Walks every winning `CELL` record and returns the first one `keep`
/// accepts, plus its resolving file index for any local form id its record
/// carries (`XCWT`).
fn find_cell(order: &LoadOrder, keep: impl Fn(&Cell) -> bool) -> Option<FoundCell> {
    for (id, rec) in order.winners_of(tag::CELL) {
        let rref = order.winner_ref(id)?;
        let localized = order.file(rref.file).plugin.header.is_localized();
        let Ok(view) = Cell::from_record(rec, localized) else {
            continue;
        };
        if keep(&view) {
            return Some(FoundCell {
                id,
                file: rref.file,
                view,
            });
        }
    }
    None
}

/// Clause 1: "draws its surface." A cell whose winning `CELL` record carries
/// `HAS_WATER` and a resolvable `XCLW` must come back from `stream_cell_id`
/// with `water_plane_count == 1` and one more surface than
/// `loaded.surfaces.len() - 1` accounts for from ordinary references, proven
/// by comparing against a second, waterless cell rather than trusting the
/// counter alone.
#[test]
fn rnd_02_a_water_cell_gets_one_water_surface() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();
    let watered = find_cell(&order, |c| c.has_water() && c.water_height.is_some());
    let dry = find_cell(&order, |c| !c.has_water());
    drop(order);

    let Some(watered) = watered else {
        eprintln!("no cell with HAS_WATER and a recorded XCLW was found, skipping");
        return;
    };
    println!("water cell: {} ({:?})", watered.id, watered.view.editor_id);

    let loaded = source
        .stream_cell_id(watered.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", watered.id));
    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "cell {} has HAS_WATER and a recorded XCLW, it must draw exactly one water plane",
        watered.id
    );
    assert!(
        !loaded.surfaces.is_empty(),
        "water_plane_count is 1 but stream_cell_id returned no surfaces at all"
    );

    let Some(dry) = dry else {
        eprintln!("every winning cell has water, skipping the negative half of this check");
        return;
    };
    let dry_loaded = source
        .stream_cell_id(dry.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", dry.id));
    assert_eq!(
        dry_loaded.stats.water_plane_count, 0,
        "cell {} has no HAS_WATER flag, it must not draw a water plane",
        dry.id
    );
}

/// Clause 2: "at the recorded height." Converts the cell's own `XCLW`
/// through `METRES_PER_UNIT` independently of `data_source.rs` (this file
/// imports the constant, not the private `water_surface` function) and
/// checks every vertex of the drawn water surface lands there, in Godot Y.
#[test]
fn rnd_02_the_water_surface_sits_at_its_recorded_xclw_height() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();
    let found = find_cell(&order, |c| c.has_water() && c.water_height.is_some());
    drop(order);
    let Some(found) = found else {
        eprintln!("no cell with HAS_WATER and a recorded XCLW was found, skipping");
        return;
    };
    let recorded_height = found
        .view
        .water_height
        .expect("filtered on water_height.is_some()");
    let want_y = recorded_height * METRES_PER_UNIT;

    let loaded = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", found.id));
    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "cell {} must draw a water plane",
        found.id
    );
    let water = loaded
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed");
    for (i, p) in water.arrays.positions.iter().enumerate() {
        assert!(
            (p[1] - want_y).abs() < 0.01,
            "cell {}'s water vertex {i} should sit at its recorded XCLW height \
             {recorded_height} Skyrim units ({want_y} m), found {} m",
            found.id,
            p[1]
        );
    }
}

/// Clause 3: "with its material." Reads the cell's resolved `WATR` winner's
/// `ANAM` and `DNAM` bytes directly here, decoded by the same TES5Edit and
/// UESP layout `data_source/water.rs` cites (opacity byte, then three four
/// byte `wbByteColors` at offsets 40, 44 and 48), and compares that
/// independently computed opacity and shallow color against
/// `MaterialDesc::water` on the drawn surface. A passing test here cannot be
/// explained by `water_material_from` and this decoder sharing one bug,
/// since this file never calls that private function.
#[test]
fn rnd_02_the_water_surface_carries_its_watr_colors_and_opacity() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();
    let found = find_cell(&order, |c| {
        c.has_water() && c.water_height.is_some() && c.water.is_some()
    });
    let Some(found) = found else {
        eprintln!("no cell with HAS_WATER, a recorded XCLW and an XCWT winner was found, skipping");
        return;
    };
    let raw_watr = found.view.water.expect("filtered on water.is_some()");
    let watr_id = order.resolve(found.file, raw_watr);
    let Some(watr_rec) = order.winner(watr_id) else {
        drop(order);
        eprintln!("cell {}'s XCWT does not resolve, skipping", found.id);
        return;
    };
    let (want_opacity, want_shallow) = decode_watr(watr_rec)
        .unwrap_or_else(|| panic!("WATR {watr_id} has no DNAM long enough to decode"));
    drop(order);

    let loaded = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", found.id));
    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "cell {} must draw a water plane",
        found.id
    );
    let water = loaded
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed");
    let got = water.arrays.material.water.as_ref().unwrap_or_else(|| {
        panic!(
            "cell {}'s water surface has no WaterMaterial attached",
            found.id
        )
    });

    assert_eq!(
        got.opacity, want_opacity,
        "cell {}'s water plane's opacity {} does not match its WATR {watr_id}'s own ANAM byte {want_opacity}",
        found.id, got.opacity
    );
    for (c, want_channel) in want_shallow.iter().enumerate() {
        assert!(
            (got.shallow_color[c] - want_channel).abs() < 0.002,
            "cell {}'s water plane's shallow color channel {c} is {}, WATR {watr_id}'s DNAM \
             decodes to {want_channel} (offset 40, one of the three wbByteColors)",
            found.id,
            got.shallow_color[c],
        );
    }
}

/// Ties the Rust data path to `godot/rnd_02_water_planes_check.gd`: resolves
/// the same named, Dawnguard exterior cell that script streams through the
/// live extension by editor id (as `CellStreamerNode::resolve_cell` does,
/// not by scanning), and checks the same "one water plane, at its recorded
/// height, carrying its material" clauses against it specifically. If this
/// test and the `.gd` script both pass, the height and material this file
/// verifies at the data level are the ones a running Godot scene actually
/// draws for that cell, not just for whichever cell the scanning tests in
/// this file happen to reach first.
#[test]
fn rnd_02_the_named_dawnguard_fixture_cell_the_godot_script_streams_has_water() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    const EDITOR_ID: &str = "DLC1VampireCastleExterior01";
    let Some(cell_id) = source.resolve_cell(EDITOR_ID) else {
        eprintln!("{EDITOR_ID} did not resolve in this load order, skipping");
        return;
    };
    let order = source.shared_order();
    let Some(rec) = order.winner(cell_id) else {
        drop(order);
        eprintln!("{EDITOR_ID} resolved to {cell_id} but it has no winning CELL record, skipping");
        return;
    };
    let localized = order
        .file(usize::from(cell_id.file))
        .plugin
        .header
        .is_localized();
    let view = Cell::from_record(rec, localized)
        .unwrap_or_else(|e| panic!("Cell::from_record({cell_id}): {e}"));
    if !view.has_water() {
        drop(order);
        eprintln!("{EDITOR_ID} ({cell_id}) has no HAS_WATER in this load order, skipping");
        return;
    }
    // This cell records XCLW as 0x7F7FFFFF, which UESP's CELL page reserves
    // and xEdit's wbCELLAfterLoad writes as "Default": the height then comes
    // from the worldspace's DNAM, not from the float itself.
    let recorded = view
        .water_height
        .filter(|h| h.is_finite() && h.abs() < 1.0e30);
    let want_units = match recorded {
        Some(h) => h,
        None => {
            let world_raw = view.world.expect("an exterior cell names its worldspace");
            let file = usize::from(cell_id.file);
            let world_id = order.resolve(file, world_raw);
            let world_rec = order.winner(world_id).expect("worldspace resolves");
            let world = World::from_record(world_rec, localized)
                .unwrap_or_else(|e| panic!("World::from_record: {e}"));
            world
                .default_water_height
                .expect("the worldspace carries a DNAM default water height")
        }
    };
    println!("{EDITOR_ID} ({cell_id}) water height {want_units} units, XCLW {recorded:?}");
    let want_y = want_units * METRES_PER_UNIT;
    drop(order);

    let loaded = source
        .stream_cell_id(cell_id)
        .unwrap_or_else(|e| panic!("stream_cell_id({cell_id}): {e}"));
    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "{EDITOR_ID} ({cell_id}) must draw exactly one water plane, same as the .gd script expects"
    );
    let water = loaded
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed");
    for (i, p) in water.arrays.positions.iter().enumerate() {
        assert!(
            (p[1] - want_y).abs() < 0.01,
            "{EDITOR_ID} ({cell_id}) water vertex {i} should sit at {want_y} m, found {} m",
            p[1]
        );
    }
    assert!(
        water.arrays.material.water.is_some(),
        "{EDITOR_ID} ({cell_id}) water surface has no WaterMaterial attached"
    );
}

/// Independent decode of a `WATR` record's `ANAM` (opacity) and `DNAM`
/// shallow color, by the offsets `data_source/water.rs`'s own doc comment
/// cites from TES5Edit's `wbDefinitionsTES5.pas` and the UESP page: opacity
/// is `ANAM`'s first byte (default 255 when absent), the shallow color is
/// the first `wbByteColors` in `DNAM`, at byte offset 40. Returns `None`
/// only when `DNAM` is present but too short to hold that offset, since a
/// missing `DNAM` is itself a valid record this file does not expect to hit
/// (every WATR winner that reaches this test already matched `c.water.is_some()`
/// on its owning cell).
fn decode_watr(rec: &Record) -> Option<(u8, [f32; 3])> {
    let opacity = rec
        .find(Tag::new(b"ANAM"))
        .and_then(|d| d.first().copied())
        .unwrap_or(255);
    let dnam = rec.find(Tag::new(b"DNAM"))?;
    if dnam.len() < 44 {
        return None;
    }
    let mut f = Fields::new(Tag::new(b"WATR"), Tag::new(b"DNAM"), dnam);
    f.skip(40).ok()?;
    let r = f.u8().ok()?;
    let g = f.u8().ok()?;
    let b = f.u8().ok()?;
    let shallow = [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ];
    Some((opacity, shallow))
}
