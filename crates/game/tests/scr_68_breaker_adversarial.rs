// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::quest;
use soe_game::{Game, Handle};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::{ObjectId, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn call_clear(game: &mut Game, alias: ObjectId) {
    let script = "Scr68Breaker";
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "ReferenceAlias")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![s("Clear"), ident("target"), ident("result")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game
        .vm
        .call_global(script, "Run", &[Value::Object(alias)])
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn package_game(alias_flags: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let quest_record = quest::build_record(0x800, "ClearQuest", 0)
        .sub(T(b"ALST"), 0u32.to_le_bytes())
        .zsub(T(b"ALID"), "Target")
        .sub(T(b"FNAM"), alias_flags.to_le_bytes())
        .sub(T(b"ALFR"), 0x802u32.to_le_bytes())
        .sub(T(b"ALPC"), 0x804u32.to_le_bytes())
        .sub(T(b"ALED"), []);
    plugin.push_top(quest_record);
    plugin.push_top(package::build_record(
        0x804,
        "AliasPackage",
        0,
        Schedule::default(),
    ));
    plugin.push_top(package::build_record(
        0x805,
        "BasePackage",
        0,
        Schedule::default(),
    ));
    plugin.push_top(
        npc::build_record(
            0x801,
            "ClearActorBase",
            "ClearActorBase",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), 0x805u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), 0x802)
            .sub(T(b"NAME"), 0x801u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let plugin = Plugin::parse(&plugin.to_bytes()).unwrap();
    let mut load_order = LoadOrder::new();
    load_order.add("Scr68Breaker.esm", plugin).unwrap();
    let mut game = Game::new(load_order);
    let reference_alias_api = ScriptBuilder::new("ReferenceAlias", "Alias")
        .function(FunctionBuilder::new("Clear", "None").native().build())
        .bytes();
    game.load_script(&reference_alias_api).unwrap();
    game
}

#[test]
fn scr_68_clear_uses_only_optional_bit_and_is_idempotent() {
    let mut game = package_game(0x0000_4002);
    let quest = game.form("ClearQuest").unwrap();
    let alias = game.object_of(Handle::Alias { quest, id: 0 });
    assert!(game.world_mut().alias_ref(quest, 0).is_some());

    call_clear(&mut game, alias);
    call_clear(&mut game, alias);

    assert!(game.vm.natives.calls.contains_key("referencealias.clear"));
    assert!(game.vm.natives.calls.contains_key("referencealias.clear"));
    assert!(!game.vm.log.contains("alias is not optional"));
    assert!(game.vm.natives.calls.contains_key("referencealias.clear"));
    assert_eq!(
        game.world_mut().quest_state(quest).unwrap().aliases.get(&0),
        Some(&None)
    );
    assert_eq!(game.world_mut().alias_ref(quest, 0), None);
    assert!(!game
        .traces()
        .iter()
        .any(|entry| entry.contains("alias is not optional")));
}

#[test]
fn scr_68_clear_does_not_treat_other_alias_flags_as_optional() {
    for flags in [0x0000_0001, 0x0000_0004, 0x0000_4080] {
        let mut game = package_game(flags);
        let quest = game.form("ClearQuest").unwrap();
        let alias = game.object_of(Handle::Alias { quest, id: 0 });
        let original = game.world_mut().alias_ref(quest, 0);

        call_clear(&mut game, alias);

        assert_eq!(
            game.world_mut().alias_ref(quest, 0),
            original,
            "flags {flags:#x}"
        );
        assert!(game
            .traces()
            .iter()
            .any(|entry| entry.contains("alias is not optional")));
    }
}

#[test]
fn scr_68_clear_switches_from_alias_package_to_base_package() {
    let mut game = package_game(quest::alias_flags::OPTIONAL);
    let quest = game.form("ClearQuest").unwrap();
    let alias = game.object_of(Handle::Alias { quest, id: 0 });
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: 0x802,
    };
    let alias_package = soe_esm::GlobalFormId {
        file: 0,
        object: 0x804,
    };
    let base_package = soe_esm::GlobalFormId {
        file: 0,
        object: 0x805,
    };
    let mut alloc = |_: &str, _: &str| ObjectId(1);

    assert_eq!(
        game.world_mut()
            .quest_state(quest)
            .unwrap()
            .def
            .alias(0)
            .unwrap()
            .flags,
        quest::alias_flags::OPTIONAL
    );
    assert!(game.world_mut().start_quest(quest, &mut alloc));
    assert_eq!(
        game.world_mut().evaluate_package(actor, &mut alloc),
        Some(alias_package)
    );

    call_clear(&mut game, alias);

    assert!(game.vm.natives.calls.contains_key("referencealias.clear"));
    assert!(!game.vm.log.contains("alias is not optional"));
    assert_eq!(
        game.world_mut().quest_state(quest).unwrap().aliases.get(&0),
        Some(&None)
    );
    assert_eq!(game.world_mut().alias_ref(quest, 0), None);
    assert_eq!(game.world_mut().current_package(actor), Some(base_package));
}
