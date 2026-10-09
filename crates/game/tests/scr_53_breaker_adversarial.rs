// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"STAT"), 0x100));
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x101));
    let mut dnam = vec![0; 100];
    dnam[0] = 7;
    plugin.push_top(
        Record::new(Tag::new(b"WEAP"), 0x200)
            .sub(Tag::new(b"DATA"), [0; 10])
            .sub(Tag::new(b"DNAM"), dnam),
    );
    let mut ammo = vec![0; 20];
    ammo[0..4].copy_from_slice(&0x204_u32.to_le_bytes());
    ammo[4..8].copy_from_slice(&(1_u32 << 2).to_le_bytes());
    ammo[8..12].copy_from_slice(&12_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"AMMO"), 0x201).sub(Tag::new(b"DATA"), ammo));
    let mut projectile = vec![0; 92];
    projectile[2..4].copy_from_slice(&(1_u16 << 6).to_le_bytes());
    projectile[4..8].copy_from_slice(&0.1_f32.to_le_bytes());
    projectile[8..12].copy_from_slice(&3000_f32.to_le_bytes());
    projectile[12..16].copy_from_slice(&5000_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"PROJ"), 0x204).sub(Tag::new(b"DATA"), projectile));
    plugin.push_top(Record::new(Tag::new(b"ARMO"), 0x202));
    plugin.push_top(
        Record::new(Tag::new(b"REFR"), 0x300).sub(Tag::new(b"NAME"), 0x100_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag::new(b"ACHR"), 0x301).sub(Tag::new(b"NAME"), 0x101_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add("SCR53-breaker.esp", plugin)
        .expect("plugin loads");
    Game::new(order)
}

fn call_fire(game: &mut Game, weapon_form: u32, source: Value, ammo: Value) -> Value {
    let script = "Scr53BreakerWeaponFireProbe";
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
    .expect("probe script loads");
    let weapon = game.object_of_form(id(weapon_form));
    game.vm
        .attach(weapon, script)
        .expect("weapon accepts probe");
    let thread = game
        .vm
        .call(weapon, "Run", &[source, ammo])
        .expect("probe calls");
    assert!(game.run_until_idle(32));
    game.vm.thread_result(thread).expect("thread completes")
}

fn form(game: &mut Game, object: u32) -> Value {
    Value::Object(game.object_of_form(id(object)))
}

#[test]
fn scr_53_actor_source_requires_its_equipped_ammunition_when_ammo_is_omitted() {
    let mut game = fixture();
    let source = form(&mut game, 0x301);
    assert_eq!(
        call_fire(&mut game, 0x200, source, Value::None),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("unsupported weapon fire behavior: source has no ranged ammunition"));
    assert!(game.world().pending.is_empty());
}

#[test]
fn scr_53_missing_ammo_form_is_reported_without_side_effects() {
    let mut game = fixture();
    let source = form(&mut game, 0x300);
    let missing_ammo = form(&mut game, 0x404);
    assert_eq!(
        call_fire(&mut game, 0x200, source, missing_ammo),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("weapon fire ammo 0:000404 is missing or is not an AMMO"));
    assert!(game.world().pending.is_empty());
    assert!(game.world().spell_casts.is_empty());
}

#[test]
fn scr_53_missing_weapon_form_is_reported_without_side_effects() {
    let mut game = fixture();
    let source = form(&mut game, 0x300);
    assert_eq!(
        call_fire(&mut game, 0x405, source, Value::None),
        Value::None
    );
    assert!(game
        .vm
        .log
        .contains("weapon fire weapon 0:000405 is missing or is not a WEAP"));
    assert!(game.world().pending.is_empty());
}

#[test]
fn scr_53_repeated_valid_requests_queue_launches_without_consuming_ammo() {
    let mut game = fixture();
    let source = form(&mut game, 0x300);
    let ammo = form(&mut game, 0x201);
    assert_eq!(
        call_fire(&mut game, 0x200, source.clone(), ammo.clone()),
        Value::None
    );
    assert_eq!(call_fire(&mut game, 0x200, source, ammo), Value::None);
    assert_eq!(
        game.vm
            .log
            .entries
            .iter()
            .filter(|entry| entry.message.contains(
                "Weapon.Fire: unsupported weapon fire behavior: ranged projectile execution"
            ))
            .count(),
        0
    );
    assert_eq!(game.world().weapon_fire_requests.len(), 2);
    assert!(game.world().pending.is_empty());
    assert!(game.world().spell_casts.is_empty());
    assert_eq!(game.world_mut().item_count(id(0x300), id(0x201)), 0);
}
