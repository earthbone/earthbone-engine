// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Additional AI-70 scale checks against the CK ObjectReference contract.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{
    conditions,
    npc::{self, template, Config, Stats},
    Game,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 24,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn game() -> Game {
    let race = Record::new(Tag(*b"RACE"), 0x810).sub(Tag(*b"DATA"), {
        let mut data = vec![0_u8; 124];
        data[16..20].copy_from_slice(&1.5_f32.to_le_bytes());
        data[20..24].copy_from_slice(&1.0_f32.to_le_bytes());
        data
    });
    let base = npc::with_height_weight(
        npc::build_record(
            0x800,
            "Actor",
            "Actor",
            Config {
                template_flags: template::TRAITS,
                level: 1,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(Tag(*b"RNAM"), 0x810_u32.to_le_bytes()),
        1.2,
        1.0,
    );
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race);
    plugin.push_top(base);
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x801).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x820));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x821).sub(Tag(*b"NAME"), 0x820_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("AI70Scale.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_70_get_scale_includes_actor_multipliers_and_reference_scale() {
    let game = game();
    game.world_mut().reference(id(0x801)).unwrap().scale = 1.25;
    game.world_mut().reference(id(0x821)).unwrap().scale = 1.25;
    let context = |subject| conditions::Context {
        owner: id(0x800),
        subject,
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
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            context(id(0x801)),
            &condition(2.25)
        ),
        Ok(true),
        "actor scale is reference scale times race height times NPC height"
    );
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            context(id(0x821)),
            &condition(1.25)
        ),
        Ok(true),
        "ordinary references report their stored scale"
    );
}
