// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition}, FileHeader, GlobalFormId, LoadOrder, Plugin,
    Record, Tag,
};
use soe_game::{conditions, inventory::*, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 723,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

#[test]
fn ai_51_item_health_percent_preserves_damaged_item_values() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x804).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai51Adversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut().reference(id(0x804)).unwrap().item = Some(
        Stack::new(
            1,
            vec![Extra {
                kind: 37,
                fields: vec![Field::FloatBits(0.75_f32.to_bits())],
            }],
        )
        .unwrap(),
    );
    let evaluate = |expected| {
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            conditions::Context {
                owner: id(0x810),
                subject: id(0x804),
                target: None,
                quest: None,
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
            },
            &condition(expected),
        )
    };
    assert_eq!(evaluate(0.75), Ok(true));
    assert_eq!(evaluate(1.0), Ok(false));
}
