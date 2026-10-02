// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MAG-13 automatic actor abilities from NPC spell lists.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallMethod, Value};

const ACTOR: u32 = 0x100;
const BASE: u32 = 0x200;
const ABILITY: u32 = 0x300;
const ORDINARY: u32 = 0x301;
const NEW_ABILITY: u32 = 0x302;
const EFFECT: u32 = 0x400;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn ability(kind: u32, form: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, kind);
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), [0; 12])
}

fn game() -> Game {
    let mut effect_data = vec![0; 152];
    put(&mut effect_data, 16, u32::MAX);
    put(&mut effect_data, 64, 0);
    put(&mut effect_data, 68, 24);
    put(&mut effect_data, 80, 0);
    put(&mut effect_data, 84, 0);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .sub(T(b"ACBS"), vec![0; 24])
            .sub(T(b"SPLO"), ABILITY.to_le_bytes())
            .sub(T(b"SPLO"), ORDINARY.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    plugin.push_top(ability(4, ABILITY));
    plugin.push_top(ability(0, ORDINARY));
    plugin.push_top(ability(4, NEW_ABILITY));
    let mut order = LoadOrder::new();
    order.add("MAG13.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn mag_13_constant_ability_is_active_when_actor_state_is_built() {
    let game = game();

    assert!(game.world_mut().actor(id(ACTOR)).is_some());

    let world = game.world();
    let effects: Vec<_> = world.active_effects().collect();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].spell, id(ABILITY));
    assert_eq!(effects[0].target, id(ACTOR));
}

#[test]
fn mag_13_ordinary_spell_is_not_automatically_active() {
    let game = game();

    game.world_mut().actor(id(ACTOR));

    assert!(game
        .world()
        .active_effects()
        .all(|effect| effect.spell != id(ORDINARY)));
}

#[test]
fn mag_13_repeated_actor_access_does_not_reapply_ability() {
    let game = game();
    game.world_mut().actor(id(ACTOR));

    game.world_mut().actor(id(ACTOR));

    assert_eq!(game.world().active_effects().count(), 1);
}

#[test]
fn mag_13_headless_spawn_ability_survives_world_save_load() {
    let game = game();
    let mut world = game.world_mut();
    assert!(world.actor(id(ACTOR)).is_some());
    let hash = world.state_hash();
    let bytes = world.save_state();

    world.load_state(&bytes).unwrap();

    assert_eq!(world.state_hash(), hash);
    assert_eq!(world.active_effects().count(), 1);
}

#[test]
fn mag_13_add_spell_native_activates_a_new_constant_ability() {
    let mut game = game();
    game.world_mut().actor(id(ACTOR));
    let run = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("actor", "Actor")
        .param("spell", "Spell")
        .local("result", "Bool")
        .ins(
            CallMethod,
            vec![
                s("AddSpell"),
                ident("actor"),
                ident("result"),
                ident("spell"),
            ],
        )
        .ins(soe_papyrus::Opcode::Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new("Mag13AddSpellProbe", "Form")
            .function(run)
            .bytes(),
    )
    .unwrap();
    let actor = Value::Object(game.object_of_form(id(ACTOR)));
    let ability = Value::Object(game.object_of_form(id(NEW_ABILITY)));
    let thread = game
        .vm
        .call_global("Mag13AddSpellProbe", "Run", &[actor, ability])
        .unwrap();
    game.run_until_idle(32);

    assert_eq!(game.vm.thread_result(thread), Some(Value::Bool(true)));
    let world = game.world();
    let active: Vec<_> = world.active_effects().collect();
    assert_eq!(active.len(), 2);
    assert!(active
        .iter()
        .any(|effect| effect.spell == id(NEW_ABILITY) && effect.target == id(ACTOR)));
}
