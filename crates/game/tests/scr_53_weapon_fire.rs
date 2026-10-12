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
    let mut dnam = vec![0; 100];
    dnam[0] = 7;
    plugin.push_top(
        Record::new(tag(*b"WEAP"), 0x200)
            .sub(tag(*b"DATA"), [0; 10])
            .sub(tag(*b"DNAM"), dnam),
    );
    plugin.push_top(Record::new(tag(*b"WEAP"), 0x203).sub(tag(*b"DATA"), [0; 10]));
    let mut ammo = vec![0; 20];
    ammo[0..4].copy_from_slice(&0x204_u32.to_le_bytes());
    ammo[4..8].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    ammo[8..12].copy_from_slice(&12_f32.to_le_bytes());
    plugin.push_top(Record::new(tag(*b"AMMO"), 0x201).sub(tag(*b"DATA"), ammo));
    let mut projectile = vec![0; 92];
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(tag(*b"PROJ"), 0x204).sub(tag(*b"DATA"), projectile));
    plugin.push_top(Record::new(tag(*b"ARMO"), 0x202));
    plugin.push_top(Record::new(tag(*b"REFR"), 0x300).sub(tag(*b"NAME"), 0x100_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("SCR53.esm", plugin).unwrap();
    Game::new(order)
}

fn call_fire(game: &mut Game, weapon: u32, source: Value, ammo: Value) -> Value {
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
    let thread = game.vm.call(weapon_object, "Run", &[source, ammo]).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).unwrap()
}

fn source_value(game: &mut Game, object: u32) -> Value {
    Value::Object(game.object_of_form(id(object)))
}

#[test]
fn scr_53_world_queues_valid_launch_and_rejects_invalid_forms() {
    let game = fixture();
    let mut world = game.world_mut();
    let source_state = world.reference(id(0x300)).unwrap();
    source_state.position = [12.0, 13.0, 14.0];
    source_state.rotation = [0.0, 0.0, std::f32::consts::FRAC_PI_2];
    drop(world);
    game.world_mut()
        .fire_weapon(id(0x200), id(0x300), Some(id(0x201)))
        .unwrap();
    let request = game.world().weapon_fire_requests[0];
    assert_eq!(request.weapon, id(0x200));
    assert_eq!(request.attacker, id(0x300));
    assert_eq!(request.ammunition, id(0x201));
    assert_eq!(request.projectile, id(0x204));
    assert_eq!(request.origin, [12.0, 13.0, 14.0]);
    assert!((request.direction[0] - 1.0).abs() < 0.0001);
    assert!(request.direction[1].abs() < 0.0001);

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
fn scr_53_weapon_fire_native_queues_valid_shot_without_damage_or_inventory_changes() {
    let mut game = fixture();
    let source = source_value(&mut game, 0x300);
    let ammo = source_value(&mut game, 0x201);
    assert_eq!(call_fire(&mut game, 0x200, source, ammo), Value::None);
    assert_eq!(game.world().weapon_fire_requests.len(), 1);
    assert!(!game.vm.log.contains("Weapon.Fire:"));
    assert!(game.world().pending.is_empty());
    assert!(game.world().spell_casts.is_empty());
    assert_eq!(game.world_mut().item_count(id(0x300), id(0x201)), 0);
}

#[test]
fn scr_53_weapon_fire_native_reports_bad_optional_ammo_argument() {
    let mut game = fixture();
    let source = source_value(&mut game, 0x300);
    assert_eq!(
        call_fire(&mut game, 0x200, source, Value::string("bad ammo")),
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
    let source = source_value(&mut game, 0x100);
    assert_eq!(
        call_fire(&mut game, 0x200, source, Value::None),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("weapon fire source is not a reference"));
    assert!(game.world().pending.is_empty());
}

#[test]
fn scr_53_weapon_fire_native_rejects_a_non_form_source_argument() {
    let mut game = fixture();
    assert_eq!(
        call_fire(&mut game, 0x200, Value::string("bad source"), Value::None),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("weapon fire source argument is not a form"));
    assert!(game.world().pending.is_empty());
}

#[test]
fn scr_53_weapon_fire_native_rejects_missing_ammo_for_reference_source() {
    let mut game = fixture();
    let source = source_value(&mut game, 0x300);
    assert_eq!(
        call_fire(&mut game, 0x200, source, Value::None),
        Value::None
    );
    assert!(game.vm.log.contains("ammunition is required"));
    assert!(game.world().pending.is_empty());
}
