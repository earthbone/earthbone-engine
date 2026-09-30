// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition}, FileHeader, GlobalFormId, LoadOrder, Plugin,
    Record, Tag,
};
use soe_game::{conditions, Game};

#[test]
fn ai_59_has_equipped_spell_accepts_voice_casting_source() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai59Adversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    let condition = Condition {
        flags: 0,
        comparison: Comparison::Number(0.0),
        function: 570,
        parameters: [2, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            conditions::Context {
                owner: GlobalFormId { file: 0, object: 0x800 },
                subject: GlobalFormId { file: 0, object: 0x900 },
                target: None,
                quest: None,
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
            },
            &condition,
        ),
        Ok(true)
    );
}
