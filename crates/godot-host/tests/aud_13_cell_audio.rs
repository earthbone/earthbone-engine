// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Cell audio precedence follows local xEdit and UESP CELL, REGN and WRLD layouts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_godot_host::audio::music_type_for_cell;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn header(kind: u32, overrides: u8, priority: u8) -> Vec<u8> {
    let mut data = kind.to_le_bytes().to_vec();
    data.extend_from_slice(&[overrides, priority, 0, 0]);
    data
}

fn world(cell_music: Option<u32>, regions: &[u32], entries: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut cell = Record::new(Tag(*b"CELL"), 0x800)
        .sub(Tag(*b"DATA"), [0])
        .sub(
            Tag(*b"XCLR"),
            regions
                .iter()
                .flat_map(|raw| raw.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    cell.placement.world = Some(0x900);
    if let Some(raw) = cell_music {
        cell = cell.sub(Tag(*b"XCMO"), raw.to_le_bytes());
    }
    plugin.push_top(cell);
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x900).sub(Tag(*b"ZNAM"), 0xA00_u32.to_le_bytes()));
    for record in entries {
        plugin.push_top(record);
    }
    for object in [0xA00, 0xB00, 0xB01, 0xB02, 0xB03] {
        plugin.push_top(Record::new(Tag(*b"MUSC"), object));
    }
    let mut order = LoadOrder::new();
    order.add("Aud13.esm", plugin).unwrap();
    World::new(order)
}

fn region(object: u32, priority: u8, music: u32) -> Record {
    Record::new(Tag(*b"REGN"), object)
        .sub(Tag(*b"RDAT"), header(7, 0, priority))
        .sub(Tag(*b"RDMO"), music.to_le_bytes())
}

#[test]
fn aud_13_cell_music_overrides_region_and_world_music() {
    let world = world(Some(0xB00), &[0x801], vec![region(0x801, 90, 0xB01)]);
    assert_eq!(
        music_type_for_cell(&world, id(0x800)).unwrap(),
        Some(id(0xB00))
    );
}

#[test]
fn aud_13_region_music_uses_priority_and_ignores_other_entry_types() {
    let ignored = Record::new(Tag(*b"REGN"), 0x803)
        .sub(Tag(*b"RDAT"), header(3, 0, 99))
        .sub(Tag(*b"RDMO"), 0xB03_u32.to_le_bytes());
    let world = world(
        None,
        &[0x801, 0x802, 0x803],
        vec![region(0x801, 1, 0xB01), region(0x802, 90, 0xB02), ignored],
    );
    assert_eq!(
        music_type_for_cell(&world, id(0x800)).unwrap(),
        Some(id(0xB02))
    );
}

#[test]
fn aud_13_equal_region_priorities_keep_cell_order() {
    let world = world(
        None,
        &[0x802, 0x801],
        vec![region(0x801, 4, 0xB01), region(0x802, 4, 0xB02)],
    );
    assert_eq!(
        music_type_for_cell(&world, id(0x800)).unwrap(),
        Some(id(0xB02))
    );
}

#[test]
fn aud_13_world_music_fills_absent_cell_and_region_music() {
    assert_eq!(
        music_type_for_cell(&world(None, &[], vec![]), id(0x800)).unwrap(),
        Some(id(0xA00))
    );
}

#[test]
fn aud_13_null_cell_music_suppresses_world_fallback() {
    assert_eq!(
        music_type_for_cell(&world(Some(0), &[], vec![]), id(0x800)).unwrap(),
        None
    );
}

#[test]
fn aud_13_regional_override_stops_lower_priority_music() {
    let overriding = Record::new(Tag(*b"REGN"), 0x802).sub(Tag(*b"RDAT"), header(7, 1, 90));
    let world = world(
        None,
        &[0x801, 0x802],
        vec![region(0x801, 1, 0xB01), overriding],
    );
    assert_eq!(
        music_type_for_cell(&world, id(0x800)).unwrap(),
        Some(id(0xA00))
    );
}

#[test]
fn aud_13_malformed_region_headers_and_music_links_return_errors() {
    for record in [
        Record::new(Tag(*b"REGN"), 0x801).sub(Tag(*b"RDAT"), [7, 0, 0, 0]),
        Record::new(Tag(*b"REGN"), 0x801).sub(Tag(*b"RDAT"), header(7, 2, 90)),
        Record::new(Tag(*b"REGN"), 0x801)
            .sub(Tag(*b"RDAT"), header(7, 0, 90))
            .sub(Tag(*b"RDMO"), [1]),
    ] {
        assert!(music_type_for_cell(&world(None, &[0x801], vec![record]), id(0x800)).is_err());
    }
}

#[test]
fn aud_13_installed_city_and_plains_music_conditions_choose_audible_and_silent_tracks() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let source =
        soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let mut game = source.localized_game().unwrap();
    let scripts = game.load_scripts_from_data(std::path::Path::new(&data));
    assert!(scripts.scripts_failed.is_empty());
    game.start();
    let player = game.player();
    let mut audible = 0;
    let mut silent = 0;
    for object in [0x01A270, 0x01A276, 0x000D74, 0x0095FC] {
        let cell = id(object);
        game.world_mut().reference(player).unwrap().cell = Some(cell);
        let sky = game.world_mut().sky_frame(cell).unwrap();
        let world = game.world();
        let data =
            soe_esm::records::cell::Cell::from_record(world.record(cell).unwrap(), false).unwrap();
        let weather = world.current_sky_weather().unwrap();
        let mask = world.record(weather).unwrap().find(Tag(*b"DATA")).unwrap()[11] & 15;
        let sounds = soe_game::ambience::descriptors_for_cell(&world, cell, mask, 0.25).unwrap();
        println!("AUD-13 cell={cell} regions={:?} space={:?} weather={weather} frame={} mask={mask} sounds={sounds:?}", data.regions, data.acoustic_space, sky.is_some());
        drop(world);
        if matches!(object, 0x01A276 | 0x0095FC) {
            assert!(
                !sounds.is_empty(),
                "vanilla {cell} has continuous regional ambience"
            );
            let plans = sounds
                .iter()
                .filter_map(|sound| {
                    let plan = soe_game::sound::prepare_for(
                        &source.shared_order(),
                        &mut game.world_mut(),
                        &game.vm,
                        *sound,
                        player,
                        None,
                        [0.25; 3],
                        &soe_game::sound::SoundSettings::default(),
                    );
                    println!("AUD-13 sound={sound} plan={plan:?}");
                    plan.ok()
                })
                .collect::<Vec<_>>();
            assert!(
                plans.iter().any(|plan| plan.looping == 8),
                "vanilla {cell} has an authored loop"
            );
        }
        let music = music_type_for_cell(&game.world(), cell).unwrap().unwrap();
        for draw in [0.0, 0.25, 0.5, 0.75, 0.99] {
            let track = soe_game::music::select(&mut game, music, player, draw).unwrap();
            assert_eq!(track.music_type, music, "cell {cell}");
            assert!(track.duration.is_finite() && track.duration >= 0.0);
            audible += usize::from(track.path.is_some());
            silent += usize::from(track.path.is_none());
            if track.path.is_none() {
                assert!(track.duration > 0.0, "authored silence has a duration");
            }
            println!(
                "AUD-13 cell={cell} music={music} track={} path={:?} duration={}",
                track.track, track.path, track.duration
            );
        }
    }
    assert!(audible > 0, "exterior music offers audible selections");
    assert!(silent > 0, "vanilla music includes authored silence");
}
