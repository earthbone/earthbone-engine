// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai66Breaker.esm", plugin).unwrap();
    Game::new(order)
}
fn eval(
    game: &Game,
    subject: GlobalFormId,
    function: u16,
    params: [u32; 2],
    expected: f32,
) -> Result<bool, conditions::Error> {
    let c = Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: params,
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
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
        },
        &c,
    )
}

#[test]
fn ai_66_flying_state_covers_all_ck_states_and_rejects_out_of_range() {
    let game = game();
    for state in 0..=4 {
        game.world_mut().actor(id(0x900)).unwrap().flying_state = Some(state);
        assert_eq!(
            eval(&game, id(0x900), 633, [0, 0], f32::from(state)),
            Ok(true)
        );
        assert_eq!(
            eval(&game, id(0x900), 633, [0, 0], f32::from((state + 1) % 5)),
            Ok(false)
        );
    }
    game.world_mut().actor(id(0x900)).unwrap().flying_state = Some(255);
    assert_eq!(
        eval(&game, id(0x900), 633, [0, 0], 0.0),
        Err(conditions::Error::StateContract(633))
    );
}

#[test]
fn ai_66_pathing_and_speed_contract_gaps_are_explicit_for_actor_context() {
    let game = game();
    assert_eq!(
        eval(&game, id(0x900), 619, [90, 0], 0.0),
        Err(conditions::Error::Contract(619))
    );
    assert_eq!(
        eval(&game, id(0x900), 623, [0, 0], 0.0),
        Err(conditions::Error::Contract(623))
    );
    assert_eq!(
        eval(&game, id(0x910), 619, [90, 0], 0.0),
        Err(conditions::Error::Form(id(0x910)))
    );
}
