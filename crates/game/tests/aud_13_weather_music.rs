// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AUD-13 weather predicates use xEdit's WTHR parameter and CommonLib's current sky identity.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in [0x800, 0x801] {
        let mut data = vec![0; 19];
        data[3] = 255;
        plugin.push_top(
            Record::new(Tag(*b"WTHR"), object)
                .sub(Tag(*b"NAM0"), vec![0; 272])
                .sub(Tag(*b"DATA"), data)
                .sub(Tag(*b"FNAM"), vec![0; 32]),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"CLMT"), 0x802)
            .sub(Tag(*b"TNAM"), [36, 48, 108, 120, 50, 0])
            .sub(
                Tag(*b"WLST"),
                [0x800_u32, 100, 0]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    plugin.push_top(Record::new(Tag(*b"WRLD"), 0x803).sub(Tag(*b"CNAM"), 0x802_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REGN"), 0x804).sub(Tag(*b"WNAM"), 0x803_u32.to_le_bytes()));
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 0x805)
            .sub(Tag(*b"DATA"), [0])
            .sub(Tag(*b"XCCM"), 0x804_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Aud13Weather.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(
    game: &Game,
    weather: u32,
    second: u32,
    expected: f32,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x805),
            subject: id(0x14),
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        &Condition {
            flags: 0,
            comparison: Comparison::Number(expected),
            function: 149,
            parameters: [weather, second],
            run_on: 0,
            reference: 0,
            parameter_3: -1,
            strings: [None, None],
        },
    )
}

#[test]
fn aud_13_weather_music_predicate_reads_selected_and_transition_weather() {
    let game = fixture();
    assert!(conditions::supports_function(149));
    assert_eq!(evaluate(&game, 0x800, 0, 0.0), Ok(true));
    game.world_mut().sky_frame(id(0x805)).unwrap();
    assert_eq!(evaluate(&game, 0x800, 0, 1.0), Ok(true));
    assert_eq!(evaluate(&game, 0x801, 0, 1.0), Ok(false));
    game.world_mut()
        .set_active_weather(id(0x801), true, false)
        .unwrap();
    assert_eq!(evaluate(&game, 0x800, 0, 1.0), Ok(true));
    game.world_mut().sky_frame(id(0x805)).unwrap();
    assert_eq!(evaluate(&game, 0x801, 0, 1.0), Ok(true));
    assert_eq!(evaluate(&game, 0x800, 0, 1.0), Ok(false));
}

#[test]
fn aud_13_weather_music_predicate_rejects_bad_forms_and_unused_parameters() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, 0x803, 0, 0.0),
        Err(conditions::Error::Form(id(0x803)))
    );
    assert_eq!(
        evaluate(&game, 0xFFF, 0, 0.0),
        Err(conditions::Error::Form(id(0xFFF)))
    );
    assert_eq!(
        evaluate(&game, 0x800, 1, 0.0),
        Err(conditions::Error::Parameter(149, 1))
    );
}

#[test]
fn aud_13_regional_fractional_chances_keep_continuous_loops_and_filter_occasional_sounds() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 0x800)
            .sub(Tag(*b"DATA"), [0])
            .sub(Tag(*b"XCLR"), 0x801_u32.to_le_bytes()),
    );
    let mut entries = Vec::new();
    for (sound, chance) in [(0x802_u32, 1.0_f32), (0x803, 0.07)] {
        entries.extend_from_slice(&sound.to_le_bytes());
        entries.extend_from_slice(&1_u32.to_le_bytes());
        entries.extend_from_slice(&chance.to_le_bytes());
    }
    plugin.push_top(
        Record::new(Tag(*b"REGN"), 0x801)
            .sub(Tag(*b"RDAT"), [7, 0, 0, 0, 0, 50, 0, 0])
            .sub(Tag(*b"RDSA"), entries),
    );
    let mut order = LoadOrder::new();
    order.add("Aud13Chance.esm", plugin).unwrap();
    let world = soe_game::World::new(order);
    for draw in [0.07, 0.25, 0.999] {
        assert_eq!(
            soe_game::ambience::descriptors_for_cell(&world, id(0x800), 1, draw).unwrap(),
            vec![id(0x802)]
        );
    }
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x800), 1, 0.05).unwrap(),
        vec![id(0x802), id(0x803)]
    );
}
