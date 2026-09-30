// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 0xA00));
    plugin.push_top(Record::new(Tag(*b"ENCH"), 0xA01));
    plugin.push_top(Record::new(Tag(*b"SHOU"), 0xA02));
    let mut order = LoadOrder::new();
    order.add("Ai76Adversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(game: &Game, form: u32, expected: f32) -> Result<bool, conditions::Error> {
    let condition = Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 493,
        parameters: [form, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: GlobalFormId {
                file: 0,
                object: 0x800,
            },
            subject: GlobalFormId {
                file: 0,
                object: 0x900,
            },
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
        &condition,
    )
}

#[test]
fn ai_76_player_knows_reads_and_saves_ck_knowable_forms() {
    let game = fixture();
    for form in [0xA00, 0xA01, 0xA02] {
        assert_eq!(evaluate(&game, form, 1.0), Ok(false));
        game.world_mut()
            .actor(GlobalFormId {
                file: 0,
                object: 0x14,
            })
            .unwrap()
            .known_forms
            .insert(GlobalFormId {
                file: 0,
                object: form,
            });
        assert_eq!(evaluate(&game, form, 1.0), Ok(true));
        assert_eq!(evaluate(&game, form, 0.0), Ok(false));
    }

    let saved = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    assert_eq!(evaluate(&restored, 0xA00, 1.0), Ok(true));
    assert_eq!(evaluate(&restored, 0xA01, 1.0), Ok(true));
    assert_eq!(evaluate(&restored, 0xA02, 1.0), Ok(true));
}
