// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x900;
const ACTOR: u32 = 0x901;
const TARGET_BASE: u32 = 0x902;
const TARGET: u32 = 0x903;
const ITEM_A: u32 = 0x904;
const ITEM_B: u32 = 0x905;
const SPELL: u32 = 0x906;
static NEXT: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), BASE));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(
                T(b"VMAD"),
                Vmad {
                    scripts: vec![ScriptEntry::new("Scr16BreakerFeedWatch")],
                    ..Default::default()
                }
                .write(T(b"ACHR")),
            ),
    );
    plugin.push_top(Record::new(T(b"NPC_"), TARGET_BASE));
    plugin.push_top(Record::new(T(b"ACHR"), TARGET).sub(T(b"NAME"), TARGET_BASE.to_le_bytes()));
    for item in [ITEM_A, ITEM_B, SPELL] {
        plugin.push_top(Record::new(T(b"MISC"), item));
    }
    let mut order = LoadOrder::new();
    order.add("Scr16Breaker.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, method: &str, args: &[Value]) {
    let script = format!("Scr16BreakerProbe{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("actor", "Actor");
    let mut operands = vec![s(method), ident("actor"), ident("::nonevar")];
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, if method == "StartCannibal" || method == "StartVampireFeed" { "Actor" } else { "Form" });
        operands.push(ident(&name));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let actor = game.object_of_form(id(ACTOR));
    let mut values = vec![Value::Object(actor)];
    values.extend_from_slice(args);
    game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
}

fn errors(game: &Game) -> Vec<String> {
    game.traces()
        .into_iter()
        .filter(|trace| trace.contains("no function") || trace.contains("error"))
        .collect()
}

#[test]
fn scr_16_breaker_unequip_all_repeats_without_removing_inventory() {
    let mut game = game();
    let actor_id = id(ACTOR);
    for item in [id(ITEM_A), id(ITEM_B)] {
        game.world_mut().add_item(actor_id, item, 1);
        game.world_mut().equip_item(actor_id, item).unwrap();
    }
    call(&mut game, "UnequipAll", &[]);
    call(&mut game, "UnequipAll", &[]);
    let mut world = game.world_mut();
    let actor = world.actor(actor_id).unwrap();
    assert!(actor.equipped.is_empty());
    assert_eq!(world.item_count(actor_id, id(ITEM_A)), 1);
    assert_eq!(world.item_count(actor_id, id(ITEM_B)), 1);
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_breaker_remove_all_spells_keeps_nonspell_equipment_on_repeat() {
    let mut game = game();
    let actor_id = id(ACTOR);
    let mut world = game.world_mut();
    let actor = world.actor(actor_id).unwrap();
    actor.spells.insert(id(SPELL));
    actor.equipped.insert(id(SPELL));
    actor.equipped.insert(id(ITEM_A));
    actor.left_spell = Some(id(SPELL));
    actor.right_spell = Some(id(SPELL));
    drop(world);
    call(&mut game, "RemoveAllSpells", &[]);
    call(&mut game, "RemoveAllSpells", &[]);
    let mut world = game.world_mut();
    let actor = world.actor(actor_id).unwrap();
    assert!(actor.spells.is_empty());
    assert!(!actor.equipped.contains(&id(SPELL)));
    assert!(actor.equipped.contains(&id(ITEM_A)));
    assert_eq!(actor.left_spell, None);
    assert_eq!(actor.right_spell, None);
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_breaker_start_cannibal_requires_a_dead_actor_target() {
    let mut game = game();
    call(&mut game, "StartCannibal", &[Value::Int(1)]);
    let target = Value::Object(game.object_of_form(id(TARGET)));
    call(
        &mut game,
        "StartCannibal",
        &[target],
    );
    assert!(game.world().animation_requests.is_empty());
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_breaker_start_vampire_feed_ignores_missing_or_awake_targets() {
    let mut game = game();
    call(&mut game, "StartVampireFeed", &[Value::Int(4)]);
    let target = Value::Object(game.object_of_form(id(TARGET)));
    call(
        &mut game,
        "StartVampireFeed",
        &[target],
    );
    assert!(!game.traces().iter().any(|trace| trace == "feed event received"));
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_breaker_clear_extra_arrows_does_not_substitute_a_graph_event_for_removal() {
    let mut game = game();
    call(&mut game, "ClearExtraArrows", &[]);
    assert!(
        game.world().animation_requests.is_empty(),
        "ClearExtraArrows must clear attached arrow 3D, not queue an unrelated graph event"
    );
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}
