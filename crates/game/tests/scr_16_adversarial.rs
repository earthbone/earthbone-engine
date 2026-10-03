// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const TARGET_BASE: u32 = 0x802;
const TARGET: u32 = 0x803;
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
                    scripts: vec![ScriptEntry::new("Scr16FeedWatch")],
                    ..Default::default()
                }
                .write(T(b"ACHR")),
            ),
    );
    plugin.push_top(Record::new(T(b"NPC_"), TARGET_BASE));
    plugin.push_top(Record::new(T(b"ACHR"), TARGET).sub(T(b"NAME"), TARGET_BASE.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Scr16.esm", plugin).unwrap();
    Game::new(order)
}

fn call(game: &mut Game, method: &str) {
    call_args(game, method, &[]);
}

fn call_args(game: &mut Game, method: &str, args: &[Value]) {
    let script = format!("Scr16Probe{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("actor", "Actor");
    let mut operands = vec![s(method), ident("actor"), ident("::nonevar")];
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, "Form");
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
    let actor_object = game.object_of_form(id(ACTOR));
    let mut values = vec![Value::Object(actor_object)];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    assert!(game.vm.thread_result(thread).is_some());
}

fn errors(game: &Game) -> Vec<String> {
    game.traces()
        .into_iter()
        .filter(|trace| {
            trace.contains("no function") || trace.contains("no global") || trace.contains("error")
        })
        .collect()
}

#[test]
fn scr_16_start_cannibal_requests_action_for_dead_actor_target() {
    let mut game = game();
    game.world_mut().actor(id(TARGET)).unwrap().dead = true;
    let target = Value::Object(game.object_of_form(id(TARGET)));
    call_args(&mut game, "StartCannibal", &[target]);
    assert_eq!(
        game.world().animation_requests,
        vec![soe_game::world::AnimationRequest {
            target: id(ACTOR),
            animation: "CannibalStart".to_owned(),
        }]
    );
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_start_cannibal_ignores_living_or_invalid_targets() {
    let mut game = game();
    let living_target = Value::Object(game.object_of_form(id(TARGET)));
    call_args(&mut game, "StartCannibal", &[living_target]);
    call_args(&mut game, "StartCannibal", &[Value::Int(10)]);
    assert!(game.world().animation_requests.is_empty());
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_clear_extra_arrows_requests_actor_graph_event() {
    let mut game = game();
    call(&mut game, "ClearExtraArrows");
    assert_eq!(
        game.world().animation_requests,
        vec![soe_game::world::AnimationRequest {
            target: id(ACTOR),
            animation: "ClearExtraArrows".to_owned(),
        }]
    );
    assert!(errors(&game).is_empty(), "{:?}", errors(&game));
}

#[test]
fn scr_16_start_vampire_feed_rejects_an_awake_target() {
    let mut game = game();
    let event = FunctionBuilder::new("OnVampireFeed", "None")
        .param("target", "Actor")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("Trace"),
                ident("::nonevar"),
                s("feed event received"),
                i(0),
            ],
        )
        .build();
    game.load_script(
        &ScriptBuilder::new("Scr16FeedWatch", "Actor")
            .function(event)
            .bytes(),
    )
    .unwrap();
    game.start();
    let probe = "Scr16FeedProbe";
    let bytes = ScriptBuilder::new(probe, "Form")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("caller", "Actor")
                .param("target", "Actor")
                .ins(
                    CallMethod,
                    vec![
                        s("StartVampireFeed"),
                        ident("caller"),
                        ident("::nonevar"),
                        ident("target"),
                    ],
                )
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let caller = game.object_of_form(id(ACTOR));
    let target = game.object_of_form(id(TARGET));
    game.vm
        .call_global(
            probe,
            "Run",
            &[Value::Object(caller), Value::Object(target)],
        )
        .unwrap();
    game.run_until_idle(64);
    assert!(
        !game
            .traces()
            .iter()
            .any(|trace| trace == "feed event received"),
        "StartVampireFeed must require the target to be sleeping"
    );
}

#[test]
fn scr_16_remove_all_spells_is_repeatable_and_keeps_nonspell_equipment() {
    let mut game = game();
    let known_spell = id(0x900);
    let unknown_spell = id(0x901);
    let armor = id(0x902);
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.spells.insert(known_spell);
        actor.equipped.extend([known_spell, unknown_spell, armor]);
        actor.left_spell = Some(known_spell);
        actor.right_spell = Some(unknown_spell);
        actor.right_weapon = Some(armor);
    }
    call(&mut game, "RemoveAllSpells");
    call(&mut game, "RemoveAllSpells");
    let world = game.world();
    let actor = world.actor_ref(id(ACTOR)).unwrap();
    assert!(actor.spells.is_empty());
    assert!(actor.equipped.contains(&armor));
    assert!(!actor.equipped.contains(&known_spell));
    assert_eq!(actor.left_spell, None);
    assert_eq!(actor.right_spell, None);
    assert_eq!(actor.right_weapon, Some(armor));
}
