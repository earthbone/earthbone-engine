// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::*, Value};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"FACT"), 1));
    plugin.push_top(Record::new(Tag::new(b"FACT"), 2));
    let mut order = LoadOrder::new();
    order.add("Factions.esm", plugin).unwrap();
    Game::new(order)
}

fn mod_reaction(game: &mut Game, amount: i32) {
    let script = format!("Scr25Mod{}", game.vm.time().to_bits());
    let bytes = ScriptBuilder::new(&script, "Faction")
        .function(
            FunctionBuilder::new("Run", "None")
                .param("other", "Faction")
                .param("amount", "Int")
                .ins(
                    CallMethod,
                    vec![
                        s("ModReaction"),
                        ident("self"),
                        ident("unused"),
                        ident("other"),
                        ident("amount"),
                    ],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let object = game.object_of_form(id(1));
    game.vm.attach(object, &script).unwrap();
    let args = [
        Value::Object(game.object_of_form(id(2))),
        Value::Int(amount),
    ];
    let thread = game.vm.call(object, "Run", &args).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn scr_25_mod_reaction_is_directional_additive_and_persistent() {
    let mut game = fixture();
    mod_reaction(&mut game, 2);
    mod_reaction(&mut game, -1);
    assert_eq!(
        game.world_mut()
            .faction(id(1))
            .unwrap()
            .reactions
            .get(&id(2)),
        Some(&1)
    );
    assert!(!game
        .world_mut()
        .faction(id(2))
        .unwrap()
        .reactions
        .contains_key(&id(1)));
    let saved = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored
            .world_mut()
            .faction(id(1))
            .unwrap()
            .reactions
            .get(&id(2)),
        Some(&1)
    );
}

#[test]
fn scr_25_mod_reaction_saturates_integer_overflow_on_repeated_calls() {
    let mut game = fixture();
    game.world_mut()
        .faction(id(1))
        .unwrap()
        .reactions
        .insert(id(2), i32::MAX - 1);
    mod_reaction(&mut game, 10);
    assert_eq!(
        game.world_mut()
            .faction(id(1))
            .unwrap()
            .reactions
            .get(&id(2)),
        Some(&i32::MAX)
    );
}
