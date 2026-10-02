// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Contract inventory and Clear behavior for Quest and reference aliases.

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::quest;
use soe_game::{natives, Game, Handle};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::{ObjectId, Value, Vm};
use std::sync::atomic::{AtomicUsize, Ordering};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

// Each binding is compared with the native lists in the corresponding CK
// class page. Individual pages are used where the local corpus has them.
const QUEST_METHODS: &[&str] = &[
    "completeallobjectives",
    "completequest",
    "failallobjectives",
    "getalias",
    "getaliasbyid",
    "getaliasbyname",
    "getcurrentstageid",
    "getid",
    "getpriority",
    "getstage",
    "getstagedone",
    "isactive",
    "iscompleted",
    "isobjectivecompleted",
    "isobjectivedisplayed",
    "isobjectivefailed",
    "isrunning",
    "isstarting",
    "isstagedone",
    "isstopping",
    "modobjectiveglobal",
    "reset",
    "setactive",
    "setcurrentstageid",
    "setobjectivecompleted",
    "setobjectivedisplayed",
    "setobjectivedisplayednowait",
    "setobjectivefailed",
    "setstage",
    "start",
    "stop",
    "updatecurrentinstanceglobal",
];

const ALIAS_METHODS: &[&str] = &[
    "clear",
    "forcerefto",
    "getactorref",
    "getactorreference",
    "getid",
    "getname",
    "getowningquest",
    "getref",
    "getreference",
];

const REFERENCE_ALIAS_METHODS: &[&str] = &[
    "addinventoryeventfilter",
    "forcerefifempty",
    "removeallinventoryeventfilters",
    "removeinventoryeventfilter",
];

fn sorted_bindings(class: &str, names: &[&str]) -> Vec<(String, String)> {
    let mut bindings: Vec<_> = names
        .iter()
        .map(|name| (class.to_ascii_lowercase(), (*name).replace(' ', "")))
        .collect();
    bindings.sort_unstable();
    bindings
}

#[test]
fn scr_68_registry_inventory_matches_ck_contract_manifest() {
    let mut vm = Vm::new();
    let world = std::rc::Rc::new(std::cell::RefCell::new(soe_game::world::World::new(
        LoadOrder::new(),
    )));
    let _ = natives::register(&mut vm, &world);

    let actual_methods: Vec<_> = vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| matches!(class.as_str(), "quest" | "alias" | "referencealias"))
        .collect();
    let expected_methods = [
        sorted_bindings("Quest", QUEST_METHODS),
        sorted_bindings("Alias", ALIAS_METHODS),
        sorted_bindings("ReferenceAlias", REFERENCE_ALIAS_METHODS),
    ]
    .concat();
    let mut expected_methods = expected_methods;
    expected_methods.sort_unstable();
    assert_eq!(actual_methods, expected_methods);

    let quest_globals: Vec<_> = vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "quest")
        .collect();
    assert_eq!(
        quest_globals,
        vec![("quest".to_string(), "getquest".to_string())]
    );
}

static PROBE_ID: AtomicUsize = AtomicUsize::new(0);

fn call_alias(game: &mut Game, alias: ObjectId, name: &str) -> Value {
    let id = PROBE_ID.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr68Probe{id}");
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var")
        .ins(CallMethod, vec![s(name), ident("target"), ident("result")])
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(function)
        .bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global(&script, "Run", &[Value::Object(alias)])
        .unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn alias_game(optional: bool) -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let alias_flags = if optional {
        quest::alias_flags::OPTIONAL
    } else {
        0
    };
    let quest_record = quest::with_alias_flags(
        quest::build_record(0x800, "ClearQuest", 0),
        0,
        "Target",
        Some(0x801),
        alias_flags,
    );
    plugin.push_top(quest_record);
    plugin.push_top(Record::new(T(b"REFR"), 0x801).zsub(T(b"EDID"), "TargetRef"));
    plugin.push_top(npc::build_record(
        0x803,
        "ClearActorBase",
        "ClearActorBase",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(
        Record::new(T(b"ACHR"), 0x802)
            .sub(T(b"NAME"), 0x803u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let bytes = plugin.to_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut load_order = LoadOrder::new();
    load_order.add("Scr68.esm", plugin).unwrap();
    Game::new(load_order)
}

#[test]
fn scr_68_clear_removes_the_reference_from_optional_aliases() {
    let mut game = alias_game(true);
    let quest = game.form("ClearQuest").unwrap();
    let alias = game.object_of(Handle::Alias { quest, id: 0 });
    assert!(game.world_mut().alias_ref(quest, 0).is_some());

    assert_eq!(call_alias(&mut game, alias, "Clear"), Value::None);

    assert_eq!(game.world_mut().alias_ref(quest, 0), None);
    assert!(!game.traces().iter().any(|entry| entry.contains("Clear:")));
}

#[test]
fn scr_68_clear_preserves_and_reports_non_optional_aliases() {
    let mut game = alias_game(false);
    let quest = game.form("ClearQuest").unwrap();
    let alias = game.object_of(Handle::Alias { quest, id: 0 });
    let original = game.world_mut().alias_ref(quest, 0);
    assert!(original.is_some());

    assert_eq!(call_alias(&mut game, alias, "Clear"), Value::None);

    assert_eq!(game.world_mut().alias_ref(quest, 0), original);
    assert!(game
        .traces()
        .iter()
        .any(|entry| entry.contains("ReferenceAlias.Clear: alias is not optional")));
}

#[test]
fn scr_68_actor_reference_getters_return_only_actor_targets() {
    let mut game = alias_game(true);
    let quest = game.form("ClearQuest").unwrap();
    let alias = game.object_of(Handle::Alias { quest, id: 0 });
    let reference = game.form("TargetRef").unwrap();

    for name in ["GetActorReference", "GetActorRef"] {
        assert_eq!(call_alias(&mut game, alias, name), Value::None, "{name}");
    }

    // Repoint this alias to an actor record and verify both shorthand forms.
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: 0x802,
    };
    game.world_mut().force_alias(quest, 0, Some(actor));
    assert_eq!(game.world_mut().class_of(Handle::Form(actor)), "Actor");
    assert_ne!(reference, actor);
    let expected = Value::Object(game.object_of(Handle::Form(actor)));
    for name in ["GetActorReference", "GetActorRef"] {
        assert_eq!(call_alias(&mut game, alias, name), expected, "{name}");
    }
}

#[test]
fn scr_68_clear_reevaluates_the_cleared_actor_package() {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let quest_record = quest::with_alias_flags(
        quest::build_record(0x800, "ClearQuest", 0),
        0,
        "Target",
        Some(0x802),
        quest::alias_flags::OPTIONAL,
    );
    plugin.push_top(quest_record);
    plugin.push_top(package::build_record(
        0x804,
        "ClearPackage",
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
        .sub(T(b"PKID"), 0x804u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), 0x802)
            .sub(T(b"NAME"), 0x801u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let plugin = Plugin::parse(&plugin.to_bytes()).unwrap();
    let mut load_order = LoadOrder::new();
    load_order.add("Scr68Actor.esm", plugin).unwrap();
    let mut game = Game::new(load_order);
    let quest = game.form("ClearQuest").unwrap();
    let alias = game.object_of(Handle::Alias { quest, id: 0 });
    let actor = soe_esm::GlobalFormId {
        file: 0,
        object: 0x802,
    };
    let package = soe_esm::GlobalFormId {
        file: 0,
        object: 0x804,
    };
    assert_eq!(game.world_mut().current_package(actor), None);

    assert_eq!(call_alias(&mut game, alias, "Clear"), Value::None);

    assert_eq!(game.world_mut().current_package(actor), Some(package));
    assert_eq!(game.world_mut().alias_ref(quest, 0), None);
}
