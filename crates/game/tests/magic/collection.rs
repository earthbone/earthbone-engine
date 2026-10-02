// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use soe_game::world::{Pending, UpdateTimer};
use soe_papyrus::Opcode::CallStatic;
use soe_papyrus::asm::{f, none};
use soe_papyrus::{NativeReturn, ObjectId};

fn callback_script(wait: bool) -> Vec<u8> {
    let mut finish = FunctionBuilder::new("OnEffectFinish", "None")
        .param("akTarget", "Actor")
        .param("akCaster", "Actor");
    if wait {
        finish = finish.ins(
            CallStatic,
            vec![s("Utility"), s("Wait"), ident("unused"), f(1.0)],
        );
    }
    ScriptBuilder::new("EffectProbe", "ActiveMagicEffect")
        .function(
            FunctionBuilder::new("OnEffectStart", "None")
                .param("akTarget", "Actor")
                .param("akCaster", "Actor")
                .ins(
                    CallStatic,
                    vec![s("Debug"), s("Trace"), ident("unused"), s("effect started")],
                )
                .build(),
        )
        .function(
            finish
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("unused"),
                        s("effect finished"),
                    ],
                )
                .build(),
        )
        .bytes()
}

fn assert_reclaimed(game: &Game, effect: EffectId, object: ObjectId) {
    assert!(game.vm.object(object).is_none());
    assert!(game.world().handle_of(object).is_none());
    assert!(
        game.world()
            .object_id(Handle::ActiveEffect(effect))
            .is_none()
    );
}

#[test]
fn mag_26_repeated_dispels_drain_unbound_callbacks_then_release_handles() {
    let mut game = fixture(0, 1, 0.0, 10);
    game.load_script(&callback_script(false)).unwrap();
    for _ in 0..64 {
        let effect = cast(&mut game, SPELL, OTHER).unwrap()[0];
        let object = game.object_of(Handle::ActiveEffect(effect));
        assert!(dispel(&mut game, effect));
        assert!(game.vm.object(object).is_some());
        assert!(game.run_until_idle(64));
        assert_reclaimed(&game, effect, object);
    }
    let traces = game.traces();
    assert_eq!(
        traces
            .iter()
            .filter(|line| line.contains("effect started"))
            .count(),
        64
    );
    assert_eq!(
        traces
            .iter()
            .filter(|line| line.contains("effect finished"))
            .count(),
        64
    );
}

#[test]
fn mag_26_expiry_preserves_sleeping_finish_across_save_load() {
    let mut game = fixture(0, 1, 0.0, 1);
    let script = callback_script(true);
    game.load_script(&script).unwrap();
    let effect = cast(&mut game, SPELL, OTHER).unwrap()[0];
    let object = game.object_of(Handle::ActiveEffect(effect));
    game.run_until_idle(64);
    game.tick(1.0);
    assert!(game.vm.object(object).is_some());
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(
        !game
            .traces()
            .iter()
            .any(|line| line.contains("effect finished"))
    );
    let saved = game.save_state().unwrap();
    let mut restored = fixture(0, 1, 0.0, 1);
    restored.load_script(&script).unwrap();
    restored.load_state(&saved).unwrap();
    restored.tick(1.0);
    assert!(
        restored
            .traces()
            .iter()
            .any(|line| line.contains("effect finished"))
    );
    assert_reclaimed(&restored, effect, object);
}

#[test]
fn mag_26_live_alias_retains_effect_until_reference_is_cleared() {
    use soe_game::quest;
    const QUEST: u32 = 0x900;
    let mut records = plugin(
        effect(0, 1, 24, false),
        vec![spell(SPELL, 0, false, 0.0, 10)],
    );
    records.push_top(quest::with_alias(
        quest::build_record(QUEST, "EffectOwner", 0),
        0,
        "Owner",
        Some(ACTOR),
    ));
    let mut game = game(records);
    let script = ScriptBuilder::new("AliasHolder", "ReferenceAlias")
        .variable("held", "ActiveMagicEffect", none())
        .bytes();
    game.load_script(&script).unwrap();
    let handle = Handle::Alias {
        quest: id(QUEST),
        id: 0,
    };
    let alias = game.object_of(handle);
    game.vm.attach(alias, "AliasHolder").unwrap();
    let effect = cast(&mut game, SPELL, OTHER).unwrap()[0];
    let object = game.object_of(Handle::ActiveEffect(effect));
    game.vm
        .set_var(alias, "AliasHolder", "held", Value::Object(object))
        .unwrap();
    assert!(dispel(&mut game, effect));
    assert!(game.run_until_idle(64));
    assert_eq!(
        game.vm.get_var(object, "EffectProbe", "finished"),
        Some(Value::Int(1))
    );
    game.vm
        .set_var(alias, "AliasHolder", "held", Value::None)
        .unwrap();
    assert!(game.run_until_idle(64));
    assert_reclaimed(&game, effect, object);
    assert_eq!(game.world().handle_of(alias), Some(handle));
    assert_eq!(game.world().object_id(handle), Some(alias));
    assert_eq!(
        game.vm.get_var(alias, "AliasHolder", "held"),
        Some(Value::None)
    );
}

#[test]
fn mag_26_pending_callback_and_late_registration_are_drained() {
    let mut game = fixture(0, 1, 0.0, 10);
    let script = ScriptBuilder::new("EffectProbe", "ActiveMagicEffect")
        .function(
            FunctionBuilder::new("OnEffectFinish", "None")
                .param("akTarget", "Actor")
                .param("akCaster", "Actor")
                .ins(
                    CallStatic,
                    vec![s("Host"), s("Queue"), ident("unused"), ident("self")],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnTail", "None")
                .ins(
                    CallStatic,
                    vec![s("Debug"), s("Trace"), ident("unused"), s("tail ran")],
                )
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let shared = game.shared_world();
    game.vm.natives.register_global(
        "Host",
        "Queue",
        Box::new(move |call| {
            let object = call.arg(0).as_object().unwrap();
            let mut world = shared.borrow_mut();
            world.pending.push(Pending::Event {
                object,
                name: "OnTail".into(),
                args: vec![],
            });
            world.timers.push(UpdateTimer {
                object,
                due: 1000.0,
                interval: None,
                game_time: false,
            });
            NativeReturn::Value(Value::None)
        }),
    );
    let effect = cast(&mut game, SPELL, OTHER).unwrap()[0];
    let object = game.object_of(Handle::ActiveEffect(effect));
    assert!(dispel(&mut game, effect));
    assert!(!game.run_until_idle(1));
    assert!(game.vm.object(object).is_some());
    assert!(game.run_until_idle(1));
    assert!(game.traces().iter().any(|line| line.contains("tail ran")));
    assert_reclaimed(&game, effect, object);
    assert!(
        game.world()
            .timers
            .iter()
            .all(|timer| timer.object != object)
    );
}
