// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Climate boundary checks for the rendered night-layer inputs.

use soe_godot_host::data_source::DataSource;
use std::path::Path;

#[test]
fn rnd_36_night_mesh_alpha_tracks_sunrise_and_sunset_boundaries() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let mut world = soe_game::World::from_shared_order(source.shared_order());
    let cell = world.cell_by_editor_id("WhiterunOrigin").unwrap();
    let climate_id = world.sky_climate_for_cell(cell).unwrap().unwrap();
    let climate = world.sky_climate(climate_id).unwrap();
    let weather = world.form("SkyrimClear").unwrap();
    let [rise_start, rise_end, set_start, set_end] = climate.timing;
    for (hour, expected) in [
        (rise_start, 1.0),
        (rise_start.midpoint(rise_end), 0.5),
        (rise_end, 0.0),
        (set_start, 0.0),
        (set_start.midpoint(set_end), 0.5),
        (set_end, 1.0),
    ] {
        let frame = world.sample_sky(weather, &climate, hour, 0.0).unwrap();
        assert!(
            (frame.night_alpha - expected).abs() < 0.00001,
            "night fade at {hour}"
        );
        assert_eq!(frame.night_model, climate.night_model);
    }
}
