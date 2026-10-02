// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-53 coverage for the `Weapon.Fire` Papyrus binding and world contract.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::FireError, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"STAT"), 0x100));
    plugin.push_top(
        Record::new(tag(*b"WEAP"), 0x200)
            .sub(tag(*b"DATA"), [0; 10])
            .sub(tag(*b"DNAM"), [0; 100]),
    );
    plugin.push_top(Record::new(tag(*b"WEAP"), 0x203).sub(tag(*b"DATA"), [0; 10]));
    plugin.push_top(Record::new(tag(*b"AMMO"), 0x201));
    plugin.push_top(Record::new(tag(*b"ARMO"), 0x202));
    plugin.push_top(Record::new(tag(*b"REFR"), 0x300).sub(tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("SCR53.esm", plugin).unwrap();
    Game::new(order)
}

fn call_fire(game: &mut Game, weapon: u32, source: u32, ammo: Value) -> Value {
    let script = "Scr53WeaponFireProbe";
    let function = FunctionBuilder::new("Run", "None")
        .param("source", "Var")
        .param("ammo", "Var")
        .ins(
            CallMethod,
            vec![
                s("Fire"),
                ident("self"),
                ident("result"),
                ident("source"),
                ident("ammo"),
            ],
        )
        .ins(Return, vec![])
        .local("result", "None")
        .build();
    game.load_script(
        &ScriptBuilder::new(script, "Weapon")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let weapon_object = game.object_of_form(id(weapon));
    game.vm.attach(weapon_object, script).unwrap();
    let source_object = game.object_of_form(id(source));
    let thread = game
        .vm
        .call(weapon_object, "Run", &[Value::Object(source_object), ammo])
        .unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn scr_53_world_validates_weapon_source_and_optional_ammo_before_rejecting_execution() {
    let game = fixture();
    let error = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)))
        .unwrap_err();
    assert!(matches!(
        error,
        FireError::Unsupported("ranged projectile execution")
    ));

    let wrong_ammo = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x202)))
        .unwrap_err();
    assert!(matches!(wrong_ammo, FireError::Ammo(form) if form == id(0x202)));

    let wrong_weapon = game
        .world_mut()
        .fire_weapon(id(0x202), id(0x300), None)
        .unwrap_err();
    assert!(matches!(wrong_weapon, FireError::Weapon(form) if form == id(0x202)));

    let malformed_weapon = game
        .world_mut()
        .fire_weapon(id(0x203), id(0x300), None)
        .unwrap_err();
    assert!(matches!(malformed_weapon, FireError::Record(_)));

    let wrong_source = game
        .world_mut()
        .fire_weapon(id(0x200), id(0x100), None)
        .unwrap_err();
    assert!(matches!(wrong_source, FireError::Source));
}

#[test]
fn scr_53_weapon_fire_native_logs_unavailable_execution_without_damage_or_events() {
    let mut game = fixture();
    let ammo = Value::Object(game.object_of_form(id(0x201)));
    assert_eq!(call_fire(&mut game, 0x200, 0x300, ammo), Value::None);
    assert!(game
        .vm
        .log
        .contains("Weapon.Fire: unsupported weapon fire behavior: ranged projectile execution"));
    assert!(game.world().pending.is_empty());
    assert!(game.world().spell_casts.is_empty());
}

#[test]
fn scr_53_weapon_fire_native_reports_bad_optional_ammo_argument() {
    let mut game = fixture();
    assert_eq!(
        call_fire(&mut game, 0x200, 0x300, Value::string("bad ammo")),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("weapon fire ammo argument is not a form"));
    assert!(game.world().pending.is_empty());
}

#[test]
fn scr_53_weapon_fire_native_rejects_a_non_reference_source() {
    let mut game = fixture();
    assert_eq!(call_fire(&mut game, 0x200, 0x100, Value::None), Value::None);
    assert!(game
        .vm
        .log
        .contains("weapon fire source is not a reference"));
    assert!(game.world().pending.is_empty());
}
