// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;

use crate::common::legacy;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn floats(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn weather(object: u32, tint: u8) -> Record {
    let mut colors = Vec::new();
    for _ in 0..17 {
        for value in [tint / 2, tint, tint / 3, tint / 10] {
            colors.extend_from_slice(&[value, value, value, 0]);
        }
    }
    let mut data = vec![0; 19];
    data[3] = 255;
    data[4] = 127;
    let mut alphas = vec![1.0_f32; 128];
    alphas[0..4].copy_from_slice(&[0.0, 0.5, 0.75, 1.0]);
    let mut xs = vec![127; 32];
    xs[0] = 0;
    xs[10] = 254;
    let mut ys = vec![127; 32];
    ys[0] = 254;
    ys[10] = 0;
    Record::new(Tag(*b"WTHR"), object)
        .sub(b"EDID", format!("Weather{object}\0").into_bytes())
        .sub(b"NAM0", colors)
        .sub(b"DATA", data)
        .sub(
            b"FNAM",
            floats(&[70.0, 7000.0, 140.0, 14000.0, 2.0, 3.0, 0.8, 0.6]),
        )
        .sub(b"00TX", b"sky/cloud0.dds\0".to_vec())
        .sub(b":0TX", b"sky/cloud10.dds\0".to_vec())
        .sub(b"10TX", b"sky/disabled.dds\0".to_vec())
        .sub(b"NAM1", 2_u32.to_le_bytes())
        .sub(b"QNAM", xs)
        .sub(b"RNAM", ys)
        .sub(b"PNAM", [255_u8; 512])
        .sub(b"JNAM", floats(&alphas))
}

fn fixture() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(weather(10, 200));
    plugin.push_top(weather(11, 100));
    plugin.push_top(
        Record::new(Tag(*b"CLMT"), 20)
            .sub(b"EDID", b"Climate\0".to_vec())
            .sub(b"TNAM", [36, 48, 108, 120, 50, 0xc3])
            .sub(b"FNAM", b"sky/sun.dds\0".to_vec())
            .sub(b"GNAM", b"sky/glare.dds\0".to_vec())
            .sub(b"MODL", b"Sky/Stars.nif\0".to_vec())
            .sub(
                b"WLST",
                [10_u32, 100, 0]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 30)
            .sub(b"EDID", b"Exterior\0".to_vec())
            .sub(b"DATA", [0_u8])
            .sub(b"XCCM", 21_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"REGN"), 21).sub(b"WNAM", 22_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"WRLD"), 22).sub(b"CNAM", 20_u32.to_le_bytes()));
    let mut light = vec![0; 92];
    light[..3].copy_from_slice(&[1, 2, 3]);
    light[20..24].copy_from_slice(&(-90_i32).to_le_bytes());
    light[24..28].copy_from_slice(&(-45_i32).to_le_bytes());
    light[88..92].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 31)
            .sub(b"DATA", [1_u8])
            .sub(b"XCLL", light)
            .sub(b"LTMP", 40_u32.to_le_bytes()),
    );
    let mut template = vec![0; 92];
    template[..3].copy_from_slice(&[60, 90, 120]);
    plugin.push_top(Record::new(Tag(*b"LGTM"), 40).sub(b"DATA", template));
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn rnd_18_time_slots_sunrise_noon_sunset_night_and_midnight_are_continuous() {
    let world = fixture();
    let climate = world.sky_climate(id(20)).unwrap();
    for (hour, expected) in [
        (0.0, 20.0),
        (6.0, 20.0),
        (7.0, 100.0),
        (8.0, 200.0),
        (12.0, 200.0),
        (18.0, 200.0),
        (19.0, 66.0),
        (20.0, 20.0),
        (24.0, 20.0),
    ] {
        let sky = world.sample_sky(id(10), &climate, hour, 0.0).unwrap();
        assert!(
            (sky.colors[0][0] * 255.0 - expected).abs() < 0.001,
            "hour {hour}"
        );
    }
}

#[test]
fn rnd_18_cloud_ascii_layers_disabled_flags_alpha_color_and_xy_speeds() {
    let world = fixture();
    let climate = world.sky_climate(id(20)).unwrap();
    let sky = world.sample_sky(id(10), &climate, 12.0, 0.0).unwrap();
    assert_eq!(sky.clouds.len(), 2);
    assert_eq!(sky.clouds[0].speed, [-0.1, 0.1]);
    assert_eq!(sky.clouds[1].speed, [0.1, -0.1]);
    assert_eq!(sky.clouds[0].alpha, 0.5);
    assert_eq!(sky.clouds[0].color, [1.0; 3]);
    assert_eq!(sky.clouds[1].texture, "sky/cloud10.dds");
}

#[test]
fn rnd_18_fog_day_night_distances_power_maximum_and_colors() {
    let world = fixture();
    let climate = world.sky_climate(id(20)).unwrap();
    let noon = world.sample_sky(id(10), &climate, 12.0, 0.0).unwrap();
    let night = world.sample_sky(id(10), &climate, 0.0, 0.0).unwrap();
    assert_eq!(noon.fog, [70.0, 7000.0, 2.0, 0.8]);
    assert_eq!(night.fog, [140.0, 14000.0, 3.0, 0.6]);
    assert!(noon.colors[1][0] > night.colors[1][0]);
}

#[test]
fn rnd_18_sun_direction_textures_moon_flags_and_eight_phases() {
    let world = fixture();
    let climate = world.sky_climate(id(20)).unwrap();
    for phase in 0..8 {
        let sky = world
            .sample_sky(id(10), &climate, 12.0, f64::from(phase * 3))
            .unwrap();
        assert_eq!(sky.moon_phase, phase);
        assert_eq!(sky.moons, [true, true]);
        assert!(sky.sun_direction[2] > 0.95);
        assert_eq!(sky.sun_texture, "sky/sun.dds");
    }
    let night = world.sample_sky(id(10), &climate, 0.0, 24.0).unwrap();
    assert_eq!(night.moon_phase, 0);
    assert!(night.sun_direction[2] < -0.95);
    assert!(night.moon_directions[0][2] > 0.99);
}

#[test]
fn rnd_18_weather_transition_uses_destination_game_hours_and_blends_clouds() {
    let mut world = fixture();
    world.clock.game_days = 0.5;
    let start = world.sky_frame(id(30)).unwrap().unwrap();
    world.set_active_weather(id(11), true, false).unwrap();
    assert_eq!(
        world.sky_frame(id(30)).unwrap().unwrap().colors,
        start.colors
    );
    world.advance_clock(22.5);
    let halfway = world.sky_frame(id(30)).unwrap().unwrap();
    assert!((halfway.transition - 0.5).abs() < 0.0001);
    assert!((halfway.colors[0][0] - 150.0 / 255.0).abs() < 0.0001);
    assert_eq!(halfway.clouds.len(), 4);
    world.advance_clock(22.5);
    let end = world.sky_frame(id(30)).unwrap().unwrap();
    assert_eq!(end.transition, 1.0);
    assert!((end.colors[0][0] - 100.0 / 255.0).abs() < 0.0001);
}

#[test]
fn rnd_18_interrupted_transition_preserves_colors_without_a_jump() {
    let mut world = fixture();
    world.clock.game_days = 0.5;
    world.sky_frame(id(30)).unwrap();
    world.set_active_weather(id(11), true, false).unwrap();
    world.sky_frame(id(30)).unwrap();
    world.advance_clock(22.5);
    let before = world.sky_frame(id(30)).unwrap().unwrap();
    world.set_active_weather(id(10), true, false).unwrap();
    assert_eq!(
        world.sky_frame(id(30)).unwrap().unwrap().colors,
        before.colors
    );
}

#[test]
fn rnd_18_accelerated_weather_uses_fastest_encoded_transition() {
    let mut world = fixture();
    world.sky_frame(id(30)).unwrap();
    world.set_active_weather(id(11), true, true).unwrap();
    world.sky_frame(id(30)).unwrap();
    world.advance_clock(1.0);
    assert_eq!(world.sky_frame(id(30)).unwrap().unwrap().transition, 1.0);
}

#[test]
fn rnd_18_interiors_keep_their_lighting_template() {
    let mut world = fixture();
    world.set_active_weather(id(10), true, false).unwrap();
    assert!(world.sky_frame(id(31)).unwrap().is_none());
    assert_eq!(
        world.interior_lighting(id(31)).unwrap().ambient,
        [60.0 / 255.0, 90.0 / 255.0, 120.0 / 255.0]
    );
    assert_eq!(
        world.interior_lighting(id(31)).unwrap().rotation,
        [-90.0, -45.0]
    );
}

#[test]
fn rnd_18_save_roundtrip_and_absent_section_restore_transition_or_default() {
    let mut world = fixture();
    world.clock.game_days = 0.5;
    world.sky_frame(id(30)).unwrap();
    world.set_active_weather(id(11), true, false).unwrap();
    world.sky_frame(id(30)).unwrap();
    world.advance_clock(22.5);
    let before = world.sky_frame(id(30)).unwrap();
    let bytes = world.save_state();
    world.advance_clock(22.5);
    world.load_state(&bytes).unwrap();
    assert_eq!(world.sky_frame(id(30)).unwrap(), before);
    let mut old = bytes;
    legacy::normalize_world_sections(&mut old);
    world.load_state(&old).unwrap();
    assert!(world.weather_request().is_none());
    assert_eq!(world.sky_frame(id(30)).unwrap().unwrap().weather, id(10));
}

#[test]
fn rnd_18_malformed_fields_return_errors_without_replacing_transition() {
    let mut world = fixture();
    world.sky_frame(id(30)).unwrap();
    let before = world.save_state();
    assert!(world.sky_climate(id(10)).is_err());
    assert_eq!(world.save_state(), before);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"WTHR"), 10).sub(b"NAM0", vec![0; 207]));
    let mut order = LoadOrder::new();
    order.add("Invalid.esm", plugin).unwrap();
    let invalid = World::new(order);
    let climate = world.sky_climate(id(20)).unwrap();
    assert!(invalid
        .sample_sky(id(10), &climate, 12.0, 0.0)
        .unwrap_err()
        .to_string()
        .contains("NAM0 size"));
    assert!(world.sample_sky(id(10), &climate, f32::NAN, 0.0).is_err());
}

#[test]
fn rnd_18_parent_worldspace_climate_inheritance_and_override_namespace() {
    let mut world = fixture();
    let mut plugin = world.order().files()[0].plugin.clone();
    plugin.push_top(Record::new(Tag(*b"WRLD"), 50).sub(b"CNAM", 20_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"WRLD"), 51)
            .sub(b"WNAM", 50_u32.to_le_bytes())
            .sub(b"PNAM", [16_u8, 0]),
    );
    let mut cell = Record::new(Tag(*b"CELL"), 32)
        .sub(b"DATA", [0_u8])
        .sub(b"XCLC", [0_u8; 8]);
    cell.placement.world = Some(51);
    plugin.push_top(cell);
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).unwrap();
    let mut patch = Plugin::new(FileHeader::default());
    patch.header.masters.push("Sky.esm".into());
    patch.push_top(weather(10, 80));
    order.add("Patch.esp", patch).unwrap();
    world = World::new(order);
    assert_eq!(world.sky_climate_for_cell(id(32)).unwrap(), Some(id(20)));
    world.clock.game_days = 0.5;
    assert!((world.sky_frame(id(32)).unwrap().unwrap().colors[0][0] - 80.0 / 255.0).abs() < 0.0001);
}

#[test]
fn rnd_18_installed_weather_climates_and_whiterun_noon_night() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let mut order = LoadOrder::new();
    for file in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                file,
                Plugin::parse(&std::fs::read(std::path::Path::new(&data).join(file)).unwrap())
                    .unwrap(),
            )
            .unwrap();
    }
    let weathers: Vec<_> = order
        .winners_of(Tag(*b"WTHR"))
        .filter(|(_, r)| !r.is_deleted())
        .map(|(id, _)| id)
        .collect();
    let climates: Vec<_> = order
        .winners_of(Tag(*b"CLMT"))
        .filter(|(_, r)| !r.is_deleted())
        .map(|(id, _)| id)
        .collect();
    let mut world = World::new(order);
    let cell = world.cell_by_editor_id("WhiterunOrigin").unwrap();
    let climate_id = world.sky_climate_for_cell(cell).unwrap().unwrap();
    let climate = world.sky_climate(climate_id).unwrap();
    for id in &weathers {
        for hour in [0.0, 7.0, 12.0, 19.0] {
            world
                .sample_sky(*id, &climate, hour, 12.0)
                .unwrap_or_else(|e| panic!("{id}: {e}"));
        }
    }
    for id in &climates {
        world
            .sky_climate(*id)
            .unwrap_or_else(|e| panic!("{id}: {e}"));
    }
    let interiors: Vec<_> = world
        .order()
        .winners_of(Tag(*b"CELL"))
        .filter(|(_, r)| {
            !r.is_deleted()
                && r.find(Tag(*b"DATA")).is_some_and(|b| b[0] & 1 != 0)
                && r.find(Tag(*b"LTMP")).is_some_and(|b| b != [0; 4])
        })
        .map(|(id, _)| id)
        .collect();
    for cell in &interiors {
        world
            .interior_lighting(*cell)
            .unwrap_or_else(|e| panic!("{cell}: {e}"));
    }
    let hour = world.form("GameHour").unwrap();
    world.set_global(hour, 12.0);
    let noon = world.sky_frame(cell).unwrap().unwrap();
    let colors = world
        .record(noon.weather)
        .unwrap()
        .find(Tag(*b"NAM0"))
        .unwrap();
    assert_eq!(
        noon.colors[0],
        std::array::from_fn(|i| f32::from(colors[4 + i]) / 255.0)
    );
    world.set_global(hour, 0.0);
    let night = world.sky_frame(cell).unwrap().unwrap();
    assert_ne!(noon.colors[0], night.colors[0]);
    println!(
        "RND-18 surveyed {} weathers, {} climates, {} template cells, Whiterun weather {}",
        weathers.len(),
        climates.len(),
        interiors.len(),
        noon.weather
    );
}

#[test]
fn rnd_18_old_cloud_aliases_and_layer_limit_preserve_authored_textures() {
    let world = fixture();
    let climate = world.sky_climate(id(20)).unwrap();
    let mut record = weather(10, 200);
    record
        .subrecords
        .retain(|s| ![Tag(*b"00TX"), Tag(*b":0TX"), Tag(*b"10TX")].contains(&s.tag));
    record = record
        .sub(b"DNAM", b"sky/legacy.dds\0".to_vec())
        .sub(b"LNAM", 4_u32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("OldSky.esm", plugin).unwrap();
    let old = World::new(order)
        .sample_sky(id(10), &climate, 12.0, 0.0)
        .unwrap();
    assert_eq!(old.clouds.len(), 1);
    assert_eq!(old.clouds[0].texture, "sky/legacy.dds");
}

#[test]
fn rnd_18_saved_weather_survives_a_later_section_failure_atomically() {
    let mut world = fixture();
    world.sky_frame(id(30)).unwrap();
    world.set_active_weather(id(11), true, false).unwrap();
    world.sky_frame(id(30)).unwrap();
    let mut bad = world.save_state();
    let mut body = bad.clone();
    legacy::strip_world_sections(&mut body);
    let start = body.len();
    let count = u16::from_le_bytes(bad[start..start + 2].try_into().unwrap());
    bad[start..start + 2].copy_from_slice(&(count + 1).to_le_bytes());
    bad.push(3);
    bad.extend_from_slice(b"bad");
    bad.extend_from_slice(&0_u32.to_le_bytes());
    world.set_active_weather(id(10), false, true).unwrap();
    let before = world.save_state();
    assert!(world
        .load_state(&bad)
        .unwrap_err()
        .to_string()
        .contains("unknown section bad at world"));
    assert_eq!(world.save_state(), before);
}

#[test]
fn rnd_18_sunrise_fog_blend_and_disabled_moons_follow_climate_timing() {
    let world = fixture();
    let mut climate = world.sky_climate(id(20)).unwrap();
    climate.moons = [false, false];
    let sky = world.sample_sky(id(10), &climate, 7.0, 0.0).unwrap();
    assert_eq!(sky.fog[0], 105.0);
    assert_eq!(sky.moons, [false, false]);
    assert_eq!(sky.night_alpha, 0.5);
}

#[test]
fn rnd_18_interior_show_sky_and_use_sky_lighting_are_independent() {
    let world = fixture();
    let mut plugin = world.order().files()[0].plugin.clone();
    for (object, flags) in [(60, 0x81_u16), (61, 0x101), (62, 0x181)] {
        plugin.push_top(
            Record::new(Tag(*b"CELL"), object)
                .sub(b"DATA", flags.to_le_bytes())
                .sub(b"XCCM", 21_u32.to_le_bytes())
                .sub(b"LTMP", 40_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.clock.game_days = 0.5;
    let own_light = world.sky_frame(id(60)).unwrap().unwrap();
    assert!(own_light.visible);
    assert_eq!(
        own_light.interior.unwrap().ambient,
        [60.0 / 255.0, 90.0 / 255.0, 120.0 / 255.0]
    );
    let no_background = world.sky_frame(id(61)).unwrap().unwrap();
    assert!(!no_background.visible);
    assert!(no_background.interior.is_none());
    let sky_light = world.sky_frame(id(62)).unwrap().unwrap();
    assert!(sky_light.visible);
    assert!(sky_light.interior.is_none());
}

#[test]
fn rnd_18_xccm_region_weather_overrides_climate_and_survives_save() {
    let world = fixture();
    let mut plugin = world.order().files()[0].plugin.clone();
    plugin.push_top(
        Record::new(Tag(*b"REGN"), 23)
            .sub(b"WNAM", 22_u32.to_le_bytes())
            .sub(b"RDAT", [3, 0, 0, 0, 1, 50, 0, 0])
            .sub(
                b"RDWT",
                [11_u32, 100, 0]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 60)
            .sub(b"DATA", 0x181_u16.to_le_bytes())
            .sub(b"XCCM", 23_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).unwrap();
    let mut world = World::new(order);
    assert_eq!(world.sky_climate_for_cell(id(60)).unwrap(), Some(id(20)));
    assert_eq!(world.sky_frame(id(60)).unwrap().unwrap().weather, id(11));
    let save = world.save_state();
    world.sky_frame(id(30)).unwrap();
    world.load_state(&save).unwrap();
    assert_eq!(world.sky_frame(id(60)).unwrap().unwrap().weather, id(11));
}

#[test]
fn rnd_18_cloud_alpha_trailing_bytes_are_rejected() {
    let world = fixture();
    let climate = world.sky_climate(id(20)).unwrap();
    let mut record = weather(10, 200);
    record
        .subrecords
        .iter_mut()
        .find(|s| s.tag == Tag(*b"JNAM"))
        .unwrap()
        .data
        .push(0);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).unwrap();
    assert!(World::new(order)
        .sample_sky(id(10), &climate, 12.0, 0.0)
        .unwrap_err()
        .to_string()
        .contains("float size"));
}

#[test]
fn rnd_18_interior_partial_templates_use_dalc_and_optional_field_defaults() {
    for len in [64, 72, 92] {
        let mut template = vec![0; len];
        template[..4].copy_from_slice(&[20, 30, 40, 0]);
        template[8..12].copy_from_slice(&[50, 60, 70, 0]);
        template[12..16].copy_from_slice(&100.0_f32.to_le_bytes());
        template[16..20].copy_from_slice(&1000.0_f32.to_le_bytes());
        template[32..36].copy_from_slice(&800.0_f32.to_le_bytes());
        template[36..40].copy_from_slice(&2.0_f32.to_le_bytes());
        let mut dalc = vec![0; 32];
        for (i, c) in dalc[..24].as_chunks_mut::<4>().0.iter_mut().enumerate() {
            c[..3].copy_from_slice(&[10 + i as u8, 20, 30]);
        }
        if len == 92 {
            template[72..76].copy_from_slice(&[80, 90, 100, 0]);
            template[76..80].copy_from_slice(&0.7_f32.to_le_bytes());
            template[80..84].copy_from_slice(&200.0_f32.to_le_bytes());
            template[84..88].copy_from_slice(&400.0_f32.to_le_bytes());
        }
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(
            Record::new(Tag(*b"LGTM"), 40)
                .sub(b"DATA", template)
                .sub(b"DALC", dalc),
        );
        plugin.push_top(
            Record::new(Tag(*b"CELL"), 31)
                .sub(b"DATA", [1])
                .sub(b"LTMP", 40_u32.to_le_bytes()),
        );
        let mut order = LoadOrder::new();
        order.add("Lighting.esm", plugin).unwrap();
        let light = World::new(order).interior_lighting(id(31)).unwrap();
        assert_eq!(light.ambient, [20.0 / 255.0, 30.0 / 255.0, 40.0 / 255.0]);
        assert_eq!(
            light.ambient_directions[5],
            [15.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0]
        );
        assert_eq!(light.fog_clip, 800.0);
        assert_eq!(light.fog[3], if len == 92 { 0.7 } else { 1.0 });
        assert_eq!(
            light.light_fade,
            if len == 92 {
                [200.0, 400.0]
            } else {
                [0.0, 0.0]
            }
        );
        assert_eq!(
            light.fog_far_color,
            if len == 92 {
                [80.0 / 255.0, 90.0 / 255.0, 100.0 / 255.0]
            } else {
                light.fog_color
            }
        );
    }
}

#[test]
fn rnd_18_interior_inheritance_keeps_local_fields_and_rejects_malformed_dalc() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut local = vec![0; 92];
    local[20..24].copy_from_slice(&(-90_i32).to_le_bytes());
    local[24..28].copy_from_slice(&(-45_i32).to_le_bytes());
    local[88..92].copy_from_slice(&1_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 31)
            .sub(b"DATA", [1])
            .sub(b"XCLL", local)
            .sub(b"LTMP", 40_u32.to_le_bytes()),
    );
    let mut light = vec![0; 92];
    light[..3].copy_from_slice(&[60, 90, 120]);
    plugin.push_top(
        Record::new(Tag(*b"LGTM"), 40)
            .sub(b"DATA", light)
            .sub(b"DALC", [100; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Sky.esm", plugin).unwrap();
    let light = World::new(order).interior_lighting(id(31)).unwrap();
    assert_eq!(light.ambient_directions, [[100.0 / 255.0; 3]; 6]);
    assert_eq!(light.rotation, [-90.0, -45.0]);

    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"LGTM"), 40)
            .sub(b"DATA", [0; 92])
            .sub(b"DALC", [0; 23]),
    );
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 31)
            .sub(b"DATA", [1])
            .sub(b"LTMP", 40_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("BadLighting.esm", plugin).unwrap();
    assert!(World::new(order)
        .interior_lighting(id(31))
        .unwrap_err()
        .to_string()
        .contains("DALC size"));
}
