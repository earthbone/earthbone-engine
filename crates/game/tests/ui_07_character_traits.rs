// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    npc::{self, Config, Stats},
    Game,
};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::CallStatic,
    Value,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn race(raw: u32, health: f32) -> Record {
    let mut data = vec![0; 128];
    data[32..36].copy_from_slice(&1u32.to_le_bytes());
    data[36..40].copy_from_slice(&health.to_le_bytes());
    data[40..44].copy_from_slice(&50.0f32.to_le_bytes());
    data[44..48].copy_from_slice(&60.0f32.to_le_bytes());
    data[48..52].copy_from_slice(&300.0f32.to_le_bytes());
    data[52..56].copy_from_slice(&1.0f32.to_le_bytes());
    Record::new(T(b"RACE"), raw)
        .zsub(T(b"EDID"), &format!("Race{raw}"))
        .zsub(T(b"FULL"), &format!("Race {raw}"))
        .sub(T(b"DATA"), data)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 90.0));
    plugin.push_top(race(0x701, 140.0));
    let config = Config {
        flags: npc::flags::FEMALE,
        ..Config::default()
    };
    plugin.push_top(
        npc::build_record(0x14, "Player", "Player", config, Stats::default())
            .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Ui07.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_07_show_race_menu_native_queues_the_host_menu_request() {
    let mut game = game();
    game.load_script(
        &ScriptBuilder::new("Ui07RaceMenuProbe", "")
            .function(
                FunctionBuilder::new("Run", "None")
                    .global()
                    .local("result", "None")
                    .ins(
                        CallStatic,
                        vec![s("Game"), s("ShowRaceMenu"), ident("result")],
                    )
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global("Ui07RaceMenuProbe", "Run", &[])
        .unwrap();
    game.run_until_idle(32);

    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
    assert_eq!(
        game.world().menu_requests,
        [soe_game::world::MenuRequest::Race]
    );
}

#[test]
fn ui_07_confirmed_race_and_sex_initialize_the_player_once() {
    let mut game = game();
    let player = game.player();
    {
        let mut world = game.world_mut();
        assert_eq!(world.player_character_traits(), Some((id(0x700), true)));
        world.set_player_character_traits(id(0x701), false).unwrap();
        assert_eq!(world.base_actor_value(player, "health"), 140.0);
        world.mod_actor_value(player, "health", 25.0).unwrap();
        world.set_player_character_traits(id(0x700), true).unwrap();
        assert_eq!(world.actor_value(player, "health"), 165.0);
        assert_eq!(
            world.actor_ref(player).and_then(|actor| actor.race),
            Some(id(0x700))
        );
        assert!(world.actor_base(id(0x14)).unwrap().female);
    }
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    let mut world = game.world_mut();
    assert_eq!(world.player_character_traits(), Some((id(0x700), true)));
    assert_eq!(world.actor_value(player, "health"), 165.0);
}

#[test]
fn ui_07_rejects_a_non_playable_or_wrong_type_race_without_mutation() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut data = vec![0; 128];
    data[36..40].copy_from_slice(&999.0f32.to_le_bytes());
    plugin.push_top(Record::new(T(b"RACE"), 0x702).sub(T(b"DATA"), data));
    plugin.push_top(Record::new(T(b"MISC"), 0x703));
    let mut order = LoadOrder::new();
    order.add("BadRace.esm", plugin).unwrap();
    // A separate load order keeps the invalid record fixture deterministic.
    let mut invalid = Game::new(order);
    let player = invalid.player();
    let before = invalid.world_mut().player_character_traits();
    assert!(invalid
        .world_mut()
        .set_player_character_traits(id(0x702), false)
        .is_err());
    assert!(invalid
        .world_mut()
        .set_player_character_traits(id(0x703), false)
        .is_err());
    assert_eq!(invalid.world_mut().player_character_traits(), before);
    assert_eq!(invalid.world_mut().actor_ref(player), None);
}
