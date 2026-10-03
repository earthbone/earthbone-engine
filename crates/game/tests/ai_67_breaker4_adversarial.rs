// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition}, FileHeader, GlobalFormId, LoadOrder, Plugin,
    Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ai67Breaker4.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(parameters: [u32; 2]) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function: 675,
        parameters,
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [Some("IsStaggering".into()), None],
    }
}

#[test]
fn ai_67_graph_integer_rejects_unused_numeric_parameter() {
    let game = fixture();
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .graph_ints
        .insert("isstaggering".into(), 1);
    let malformed = condition([0, 9]);
    assert_eq!(
        conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            conditions::Context {
                owner: id(0x800),
                subject: id(0x900),
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
            &malformed,
        ),
        Err(conditions::Error::Parameter(675, 9))
    );
}
