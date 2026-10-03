// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! SCR-69 checks the Scene and Scroll native inventory against their script
//! contracts documented in `docs/game.md`.

use soe_esm::LoadOrder;
use soe_esm::{FileHeader, GlobalFormId, Plugin, Record, Tag};
use soe_game::quest;
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::{ObjectId, Value};

type Bindings = Vec<(String, String)>;
const QUEST: u32 = 0x800;
const ACTIVE_SCENE: u32 = 0x801;
const FORCED_SCENE: u32 = 0x802;
const ACTOR: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn scene_scroll_bindings(game: &Game) -> (Bindings, Bindings) {
    let methods = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| class == "scene" || class == "scroll");
    let globals = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "scene" || class == "scroll");
    (methods.collect(), globals.collect())
}

fn force_start_fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(quest::with_alias(
        quest::build_record(QUEST, "SceneQuest", 0),
        7,
        "Participant",
        None,
    ));
    for (form, name) in [(ACTIVE_SCENE, "ActiveScene"), (FORCED_SCENE, "ForcedScene")] {
        plugin.push_top(
            Record::new(Tag(*b"SCEN"), form)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"HNAM"), [])
                .zsub(Tag(*b"NAM0"), "OnlyPhase")
                .sub(Tag(*b"ALID"), 7i32.to_le_bytes())
                .sub(Tag(*b"PNAM"), QUEST.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR));
    let mut order = LoadOrder::new();
    order.add("Scr69.esp", plugin).unwrap();
    Game::new(order)
}

fn call_scene_method(game: &mut Game, target: Value, method: &str) -> Value {
    let script_name = format!("Scr69{method}Probe");
    let function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var")
        .ins(
            CallMethod,
            vec![s(method), ident("target"), ident("result")],
        )
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script_name, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let thread = game.vm.call_global(&script_name, "Run", &[target]).unwrap();
    game.run_until_idle(32);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_69_registry_inventory_matches_scene_and_scroll_script_contracts() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();

    let (methods, globals) = scene_scroll_bindings(&game);
    assert_eq!(
        methods,
        [
            ("scene".into(), "forceStart".to_ascii_lowercase()),
            ("scene".into(), "getOwningQuest".to_ascii_lowercase()),
            ("scene".into(), "isActionComplete".to_ascii_lowercase()),
            ("scene".into(), "isPlaying".to_ascii_lowercase()),
            ("scene".into(), "start".to_ascii_lowercase()),
            ("scene".into(), "stop".to_ascii_lowercase()),
            ("scroll".into(), "cast".into()),
        ],
        "registry methods must stay closed over the seven vanilla PSC declarations"
    );
    assert!(
        globals.is_empty(),
        "Scene.psc and Scroll.psc declare no global bindings"
    );
}

#[test]
fn scr_69_contract_matrix_covers_every_registered_method_and_finding_owner() {
    let audit = include_str!("../../../docs/game.md");
    let entries = [
        "| `Scene.Start` |",
        "| `Scene.ForceStart` |",
        "| `Scene.Stop` |",
        "| `Scene.IsPlaying` |",
        "| `Scene.GetOwningQuest` |",
        "| `Scene.IsActionComplete` |",
        "| `Scroll.Cast` |",
    ];
    for entry in entries {
        assert!(
            audit.contains(entry),
            "missing contract comparison for {entry}"
        );
    }
    assert!(audit.contains("seven methods and no globals"));
    assert!(audit.contains("active scene sharing a resolved reference"));
    assert!(audit.contains("MAG-18"));
}

#[test]
fn scr_69_forcestart_ends_an_active_scene_sharing_a_participant() {
    let mut game = force_start_fixture();
    let active = id(ACTIVE_SCENE);
    let forced = id(FORCED_SCENE);
    assert!(game.world_mut().force_alias(id(QUEST), 7, Some(id(ACTOR))));
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(active, &mut allocate));

    let target = Value::Object(game.object_of_form(forced));
    assert_eq!(
        call_scene_method(&mut game, target, "ForceStart"),
        Value::None
    );
    assert!(!game.world().scenes.contains(&active));
    assert!(game.world().scenes.contains(&forced));
}
