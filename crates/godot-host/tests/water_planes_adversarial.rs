// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial tests for RND-02 (render water planes), reached through the
//! public `DataSource::stream_cell_id` since `DataSource::water_surface` and
//! the rest of the new water plumbing in `data_source.rs` and
//! `data_source/water.rs` are private to the crate. These attack the done
//! check ("a cell with water draws its surface at the recorded height with
//! its material") against an installed Skyrim SE copy, not the synthetic
//! happy path.
//!
//! Run with SKYRIM_DATA set, headless, no windowed renderer needed:
//! ```text
//! SKYRIM_DATA="/path/to/Skyrim Special Edition/Data" cargo test -p soe-godot-host --release --test water_planes_adversarial -- --nocapture
//! ```

use std::path::PathBuf;

use soe_esm::records::cell::Cell;
use soe_esm::records::wrld::World;
use soe_esm::{tag, GlobalFormId, LoadOrder};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::units::{CELL_METRES, METRES_PER_UNIT};

fn open_data() -> Option<DataSource> {
    let data = std::env::var_os("SKYRIM_DATA").map(PathBuf::from)?;
    Some(DataSource::open(&data).unwrap_or_else(|e| panic!("DataSource::open: {e}")))
}

/// A parsed `CELL` winner plus the file index its record came from, needed
/// to resolve any local form id (`XCWT`, the group tree's `world`) the
/// record carries.
struct FoundCell {
    id: GlobalFormId,
    file: usize,
    view: Cell,
}

/// Walks every winning `CELL` record and returns the first one `keep`
/// accepts. Centralises the `winner_ref` and localisation lookups every
/// scan below needs, so a scan is one filter closure, not a copy of this
/// loop.
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

/// UESP's `CELL` page (one of this row's read-first sources) is explicit:
/// "XCLW: Non-ocean water-height in cell, is used for rivers, ponds etc.,
/// ocean-water is globally defined elsewhere." An exterior cell can
/// therefore carry the `HAS_WATER` flag with no `XCLW` of its own, relying
/// on its worldspace's `WRLD` `DNAM` default water height instead (`esm`
/// already parses this as `World::default_water_height`). `water_surface`
/// never reads the cell's worldspace: `let height = view.water_height?`
/// returns `None` the moment `XCLW` is absent, so every such cell, which in
/// practice is most of the open ocean around Tamriel and Solstheim, streams
/// with `water_plane_count == 0` and no water surface at all.
#[test]
fn a_water_cell_with_no_recorded_height_still_draws_the_worldspaces_default_water() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();

    let found = find_cell(&order, |c| {
        c.grid.is_some() && c.has_water() && c.water_height.is_none() && c.world.is_some()
    });
    let Some(found) = found else {
        eprintln!("no exterior water cell without its own XCLW was found, skipping");
        return;
    };

    let world_raw = found.view.world.expect("filtered on world.is_some()");
    let world_id = order.resolve(found.file, world_raw);
    let Some(world_rec) = order.winner(world_id) else {
        eprintln!("cell {}'s worldspace does not resolve, skipping", found.id);
        return;
    };
    let localized = order.file(found.file).plugin.header.is_localized();
    let world = World::from_record(world_rec, localized)
        .unwrap_or_else(|e| panic!("World::from_record: {e}"));
    let Some(default_height) = world.default_water_height else {
        eprintln!(
            "cell {}'s worldspace has no DNAM default water, skipping",
            found.id
        );
        return;
    };
    drop(order);

    let loaded = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", found.id));

    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "cell {} has HAS_WATER set and no XCLW, its water height comes from its \
         worldspace's DNAM default per UESP's CELL page (\"ocean-water is globally \
         defined elsewhere\"), it must still draw a water plane",
        found.id
    );
    let water = loaded
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed");
    let want_y = default_height * METRES_PER_UNIT;
    let got_y = water.arrays.positions[0][1];
    assert!(
        (got_y - want_y).abs() < 0.05,
        "cell {}'s water plane should sit at its worldspace's default water height \
         {default_height} Skyrim units ({want_y} m), found {got_y} m",
        found.id
    );
}

/// The contrapositive of the done check: a cell without the `HAS_WATER`
/// flag must never grow a water plane, whatever an overridden or a
/// lower-priority plugin's `CELL` record for the same form said. Guards the
/// winning-record flag check `water_surface` and the `stream_cell_id`
/// caller wire together against a false positive.
#[test]
fn a_cell_without_the_has_water_flag_draws_no_water_plane() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();

    let found = find_cell(&order, |c| !c.has_water());
    let Some(found) = found else {
        eprintln!("every winning cell has water, skipping");
        return;
    };
    drop(order);

    let loaded = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", found.id));

    assert_eq!(
        loaded.stats.water_plane_count, 0,
        "cell {} has no HAS_WATER flag, it must not get a water plane",
        found.id
    );
}

/// The done check itself: "a cell with water draws its surface at the
/// recorded height." An interior cell's water has no `XCLC` grid, so its
/// height is the one direct, unambiguous number `water_surface` has: its own
/// `XCLW`. Converting that Skyrim-unit height into Godot's Y through
/// `METRES_PER_UNIT` must land within float rounding of the recorded value,
/// not off by a unit-scale factor or an axis swap.
#[test]
fn an_interior_water_plane_sits_at_its_recorded_xclw_height() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();

    let found = find_cell(&order, |c| {
        c.is_interior() && c.has_water() && c.water_height.is_some()
    });
    let Some(found) = found else {
        eprintln!("no interior water cell was found, skipping");
        return;
    };
    drop(order);
    let recorded_height = found
        .view
        .water_height
        .expect("filtered on water_height.is_some()");

    let loaded = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", found.id));

    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "cell {} has HAS_WATER and a recorded XCLW, it must draw a water plane",
        found.id
    );
    let water = loaded
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed");
    let want_y = recorded_height * METRES_PER_UNIT;
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

/// `water_plane_mesh`'s own doc comment says an exterior water plane is
/// "sized to the exterior grid square (from XCLC)", one `CELL_UNITS` (4096
/// Skyrim unit, `CELL_METRES` in Godot) square at the cell's own grid
/// position. Checked independently here: a coordinate swap or an off by one
/// cell in either axis would still draw *a* plane and still pass the first
/// two tests above, only its footprint would be wrong, silently misaligning
/// every river and lake surface with the terrain beneath it.
#[test]
fn an_exterior_water_plane_spans_exactly_its_own_recorded_grid_cell() {
    let Some(source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();

    let found = find_cell(&order, |c| {
        c.grid.is_some() && c.has_water() && c.water_height.is_some()
    });
    let Some(found) = found else {
        eprintln!("no exterior water cell with its own XCLW was found, skipping");
        return;
    };
    drop(order);
    let grid = found.view.grid.expect("filtered on grid.is_some()");

    let loaded = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}): {e}", found.id));
    assert_eq!(
        loaded.stats.water_plane_count, 1,
        "cell {} has HAS_WATER and a recorded XCLW, it must draw a water plane",
        found.id
    );
    let water = loaded
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed");

    let want_min_x = grid.x as f32 * CELL_METRES;
    let want_max_x = want_min_x + CELL_METRES;
    // Godot Z is -Skyrim Y, so the grid cell's northern (larger Y) edge is
    // its smaller (more negative) Godot Z edge.
    let want_min_z = -(grid.y as f32 + 1.0) * CELL_METRES;
    let want_max_z = -(grid.y as f32) * CELL_METRES;

    let bounds = water.arrays.bounds;
    assert!(
        (bounds.min.x - want_min_x).abs() < 0.05 && (bounds.max.x - want_max_x).abs() < 0.05,
        "cell {} at grid ({}, {}) should span Godot X [{want_min_x}, {want_max_x}], \
         found [{}, {}]",
        found.id,
        grid.x,
        grid.y,
        bounds.min.x,
        bounds.max.x
    );
    assert!(
        (bounds.min.z - want_min_z).abs() < 0.05 && (bounds.max.z - want_max_z).abs() < 0.05,
        "cell {} at grid ({}, {}) should span Godot Z [{want_min_z}, {want_max_z}], \
         found [{}, {}]",
        found.id,
        grid.x,
        grid.y,
        bounds.min.z,
        bounds.max.z
    );
}

/// The row's own aim list calls out "budgets that are only met on a warm
/// cache" and "repeated open and close cycles". `real_cell.rs` already
/// checks that ordinary surfaces stay identical across worker counts on a
/// warm cache, but never looks at `water_plane_count` or the water
/// surface itself, so a thread-count-dependent water bug (the interior
/// footprint's bounds union running over a `surfaces` vector whose build
/// order can vary with the worker count, or a plane built once but wired to
/// the wrong reference on a later call) would currently pass the whole
/// suite unnoticed.
#[test]
fn the_water_plane_is_identical_across_worker_counts_on_a_warm_cache() {
    let Some(mut source) = open_data() else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = source.shared_order();

    let found = find_cell(&order, |c| c.has_water() && c.water_height.is_some());
    let Some(found) = found else {
        eprintln!("no water cell with its own XCLW was found, skipping");
        return;
    };
    drop(order);

    source.set_threads(1);
    let first = source
        .stream_cell_id(found.id)
        .unwrap_or_else(|e| panic!("stream_cell_id({}) at 1 worker: {e}", found.id));
    assert_eq!(
        first.stats.water_plane_count, 1,
        "cell {} must have a water plane",
        found.id
    );
    let first_water = first
        .surfaces
        .last()
        .expect("water_plane_count is 1 but no surface was pushed")
        .clone();

    for workers in [2, 8] {
        source.set_threads(workers);
        let again = source
            .stream_cell_id(found.id)
            .unwrap_or_else(|e| panic!("stream_cell_id({}) at {workers} workers: {e}", found.id));
        assert_eq!(
            again.stats.water_plane_count, 1,
            "cell {} must have a water plane at {workers} workers too, not just at 1",
            found.id
        );
        let again_water = again
            .surfaces
            .last()
            .expect("water_plane_count is 1 but no surface was pushed");
        assert_eq!(
            again_water.reference, first_water.reference,
            "worker count {workers} changed the water plane's owning reference"
        );
        assert_eq!(
            again_water.arrays, first_water.arrays,
            "worker count {workers} changed the water plane's geometry or material"
        );
    }
}
