// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use soe_papyrus::Value;

fn set_god_mode(game: &mut Game, enabled: bool) {
    let script = "Scr21BreakerGodMode";
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("enabled", "Bool")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("SetGodMode"),
                ident("::nonevar"),
                ident("enabled"),
            ],
        )
        .ins(Return, vec![soe_papyrus::asm::none()])
        .build();
    game.load_script(
        &ScriptBuilder::new(script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm
        .call_global(script, "Run", &[Value::Bool(enabled)])
        .unwrap();
    game.run_until_idle(64);
}

#[test]
fn scr_21_native_god_mode_blocks_only_player_health_damage() {
    let mut game = Game::new(LoadOrder::new());
    let player = game.player();
    set_god_mode(&mut game, true);
    {
        let mut world = game.world_mut();
        let health = world.actor(player).unwrap().values.get("health").current();
        let stamina = world.actor(player).unwrap().values.get("stamina").current();
        world.damage_actor_value(player, "health", 20.0).unwrap();
        world.damage_actor_value(player, "stamina", 20.0).unwrap();
        assert_eq!(world.actor(player).unwrap().values.get("health").current(), health);
        assert_eq!(
            world.actor(player).unwrap().values.get("stamina").current(),
            stamina - 20.0
        );
    }
    set_god_mode(&mut game, false);
    let mut world = game.world_mut();
    let health = world.actor(player).unwrap().values.get("health").current();
    world.damage_actor_value(player, "health", 20.0).unwrap();
    assert_eq!(world.actor(player).unwrap().values.get("health").current(), health - 20.0);
}

#[test]
fn scr_21_god_mode_does_not_protect_an_actor_when_it_is_not_the_player() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Scr21.esm", plugin).unwrap();
    let mut game = Game::new(order);
    set_god_mode(&mut game, true);
    let mut world = game.world_mut();
    let actor = GlobalFormId { file: 0, object: 0x900 };
    let health = world.actor(actor).unwrap().values.get("health").current();
    world.damage_actor_value(actor, "health", 20.0).unwrap();
    assert_eq!(world.actor(actor).unwrap().values.get("health").current(), health - 20.0);
}

#[test]
fn scr_21_channel_notify_keeps_empty_and_whitespace_arguments() {
    let mut game = Game::new(LoadOrder::new());
    let script = "Scr21BreakerNotify";
    let function = FunctionBuilder::new("Run", "None")
        .global()
        .param("channel", "String")
        .param("message", "String")
        .ins(
            CallStatic,
            vec![
                s("Debug"),
                s("DebugChannelNotify"),
                ident("::nonevar"),
                ident("channel"),
                ident("message"),
            ],
        )
        .ins(Return, vec![soe_papyrus::asm::none()])
        .build();
    game.load_script(
        &ScriptBuilder::new(script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    game.vm
        .call_global(script, "Run", &[Value::string(""), Value::string(" line\n")])
        .unwrap();
    game.run_until_idle(64);
    assert!(game.vm.log.contains("[]  line\n"));
}
