// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AUD-13 record links and region ambience follow the UESP CELL and REGN layouts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_godot_host::audio::music_type_for_cell;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn world(cell_flags: u8, cell_music: Option<u32>, regions: &[u32], entries: Vec<Record>) -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut cell = Record::new(Tag(*b"CELL"), 0x800)
        .sub(Tag(*b"DATA"), [cell_flags])
        .sub(
            Tag(*b"XCLR"),
            regions
                .iter()
                .flat_map(|raw| raw.to_le_bytes())
                .collect::<Vec<_>>(),
        );
    if let Some(raw) = cell_music {
        cell = cell.sub(Tag(*b"XCMO"), raw.to_le_bytes());
    }
    plugin.push_top(cell);
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x900).sub(Tag(*b"ZNAM"), 0xA00_u32.to_le_bytes()));
    for entry in entries {
        plugin.push_top(entry);
    }
    let mut order = LoadOrder::new();
    order.add("Aud13Adversarial.esm", plugin).unwrap();
    World::new(order)
}

fn sound_region(object: u32, rows: &[(u32, u32, f32)]) -> Record {
    let mut data = Vec::new();
    for (sound, flags, chance) in rows {
        data.extend_from_slice(&sound.to_le_bytes());
        data.extend_from_slice(&flags.to_le_bytes());
        data.extend_from_slice(&chance.to_le_bytes());
    }
    Record::new(Tag(*b"REGN"), object)
        .sub(Tag(*b"RDAT"), [7, 0, 0, 0, 0, 10, 0, 0])
        .sub(Tag(*b"RDSA"), data)
}

fn sound(object: u32) -> Record {
    Record::new(Tag(*b"SNDR"), object)
}

#[test]
fn aud_13_dangling_nonzero_cell_music_link_is_an_error() {
    let world = world(0, Some(0xBAD), &[], vec![]);
    assert!(music_type_for_cell(&world, id(0x800)).is_err());
}

#[test]
fn aud_13_dangling_regional_music_link_does_not_fall_through_to_world_music() {
    let region = Record::new(Tag(*b"REGN"), 0x801)
        .sub(Tag(*b"RDAT"), [7, 0, 0, 0, 0, 10, 0, 0])
        .sub(Tag(*b"RDMO"), 0xBAD_u32.to_le_bytes());
    let world = world(0, None, &[0x801], vec![region]);
    assert!(music_type_for_cell(&world, id(0x800)).is_err());
}

#[test]
fn aud_13_regional_ambience_obeys_weather_mask_and_percent_chance() {
    let world = world(
        0,
        None,
        &[0x801],
        vec![
            sound_region(
                0x801,
                &[(0x802, 1, 100.0), (0x804, 2, 100.0), (0x806, 0, 0.0)],
            ),
            sound(0x802),
            sound(0x804),
            sound(0x806),
        ],
    );
    let selected = soe_game::ambience::descriptors_for_cell(&world, id(0x800), 1, 0.5).unwrap();
    assert_eq!(selected, vec![id(0x802)]);
    let selected = soe_game::ambience::descriptors_for_cell(&world, id(0x800), 2, 0.5).unwrap();
    assert_eq!(selected, vec![id(0x804)]);
}

#[test]
fn aud_13_exterior_regions_and_interior_acoustic_space_regions_apply_by_cell_type() {
    let region = sound_region(0x802, &[(0x803, 0, 100.0)]);
    let exterior = world(0, None, &[0x802], vec![region.clone(), sound(0x803)]);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&exterior, id(0x800), 1, 0.5).unwrap(),
        vec![id(0x803)]
    );

    let acoustic = Record::new(Tag(*b"ASPC"), 0x801).sub(Tag(*b"RDAT"), 0x802_u32.to_le_bytes());
    let interior = world(
        1,
        None,
        &[],
        vec![
            Record::new(Tag(*b"CELL"), 0x804)
                .sub(Tag(*b"DATA"), [1])
                .sub(Tag(*b"XCAS"), 0x801_u32.to_le_bytes()),
            acoustic,
            region,
            sound(0x803),
        ],
    );
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&interior, id(0x804), 1, 0.5).unwrap(),
        vec![id(0x803)]
    );
}
