// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial ambient descriptor cases from ASPC and REGN records.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn region_world(interior: bool, flags: u32, chance: f32) -> soe_game::World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000)
            .sub(T(b"DATA"), [u8::from(interior), 0])
            .sub(T(b"XCAS"), 0x1001u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ASPC"), 0x1001)
            .sub(T(b"SNAM"), 0x1002u32.to_le_bytes())
            .sub(T(b"RDAT"), 0x1003u32.to_le_bytes()),
    );
    let mut entry = Vec::new();
    entry.extend_from_slice(&0x1004u32.to_le_bytes());
    entry.extend_from_slice(&flags.to_le_bytes());
    entry.extend_from_slice(&chance.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"REGN"), 0x1003)
            .sub(T(b"RDAT"), [7, 0, 0, 0, 0, 1, 0, 0])
            .sub(T(b"RDSA"), entry),
    );
    plugin.push_top(Record::new(T(b"SNDR"), 0x1002));
    plugin.push_top(Record::new(T(b"SNDR"), 0x1004));
    let mut order = LoadOrder::new();
    order.add("AUD11-breaker.esm", plugin).unwrap();
    soe_game::World::new(order)
}

#[test]
fn aud_11_breaker_aspc_region_sound_is_interior_only() {
    let exterior = region_world(false, 1, 100.0);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&exterior, id(0x1000), 1, 0.0).unwrap(),
        [id(0x1002)]
    );

    let interior = region_world(true, 1, 100.0);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&interior, id(0x1000), 1, 0.0).unwrap(),
        [id(0x1002), id(0x1004)]
    );
}

#[test]
fn aud_11_breaker_regional_weather_bits_and_chance_boundary_are_exact() {
    for weather in [1u8, 2, 4, 8] {
        let world = region_world(true, u32::from(weather), 50.0);
        assert!(
            soe_game::ambience::descriptors_for_cell(&world, id(0x1000), weather, 0.499)
                .unwrap()
                .contains(&id(0x1004)),
            "weather bit {weather} should admit its matching entry"
        );
        assert!(
            !soe_game::ambience::descriptors_for_cell(&world, id(0x1000), weather, 0.5)
                .unwrap()
                .contains(&id(0x1004)),
            "a 50 percent entry excludes the exact 0.5 draw"
        );
        let other = if weather == 1 { 8u8 } else { 1 };
        assert!(
            !soe_game::ambience::descriptors_for_cell(&world, id(0x1000), other, 0.1)
                .unwrap()
                .contains(&id(0x1004)),
            "weather bit {other} must not admit bit {weather}"
        );
    }
}

#[test]
fn aud_11_breaker_reserved_regional_weather_flags_are_rejected() {
    let world = region_world(true, 0x10, 100.0);
    assert!(soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.0).is_err());
}
