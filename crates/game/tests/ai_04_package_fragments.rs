// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::package::{self, Schedule};
use soe_game::vmad::{PackFragment, PackFragments, ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, IAdd};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const ACTOR: u32 = 0x801;
const FIRST: u32 = 0x802;
const SECOND: u32 = 0x803;

fn id(form: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId {
        file: 0,
        object: form,
    }
}

fn fragment(function: &str) -> PackFragment {
    PackFragment {
        marker: 1,
        script: "PackFragments".into(),
        function: function.into(),
    }
}

fn package(form: u32, name: &str, schedule: Schedule, callbacks: bool) -> Record {
    let mut record = package::build_record(form, name, 0, schedule);
    if callbacks {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("PackFragments")],
            pack_fragments: Some(PackFragments {
                version: 2,
                file: "PackFragments".into(),
                begin: Some(fragment("OnBegin")),
                end: Some(fragment("OnEnd")),
                change: Some(fragment("OnChange")),
            }),
            ..Vmad::default()
        };
        record = record.sub(T(b"VMAD"), vmad.write(T(b"PACK")));
    }
    record
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "FragmentActor")
            .sub(T(b"PKID"), FIRST.to_le_bytes())
            .sub(T(b"PKID"), SECOND.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "FragmentActorRef")
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    plugin.push_top(package(
        FIRST,
        "FirstPackage",
        Schedule {
            hour: 0,
            duration_minutes: 60,
            ..Schedule::default()
        },
        true,
    ));
    plugin.push_top(package(
        SECOND,
        "SecondPackage",
        Schedule {
            hour: 1,
            duration_minutes: 60,
            ..Schedule::default()
        },
        true,
    ));
    plugin
}

fn fragment_script() -> Vec<u8> {
    let callback = |function: &str, counter: &str, actor_var: &str| {
        FunctionBuilder::new(function, "None")
            .param("akActor", "Actor")
            .ins(IAdd, vec![ident(counter), ident(counter), i(1)])
            .ins(Assign, vec![ident(actor_var), ident("akActor")])
            .build()
    };
    ScriptBuilder::new("PackFragments", "Package")
        .variable("begins", "Int", i(0))
        .variable("ends", "Int", i(0))
        .variable("changes", "Int", i(0))
        .variable("beginActor", "Actor", none())
        .variable("endActor", "Actor", none())
        .variable("changeActor", "Actor", none())
        .function(callback("OnBegin", "begins", "beginActor"))
        .function(callback("OnEnd", "ends", "endActor"))
        .function(callback("OnChange", "changes", "changeActor"))
        .bytes()
}

#[test]
fn ai_04_package_begin_change_and_end_fragments_run_once_on_their_boundaries() {
    let mut order = LoadOrder::new();
    order.add("Ai04.esm", plugin()).unwrap();
    let mut game = Game::new(order);
    game.load_script(&fragment_script()).unwrap();
    game.bind();

    let actor = id(ACTOR);
    let package_object = game.object_of_form(id(FIRST));
    let second_package_object = game.object_of_form(id(SECOND));
    game.world_mut().clock.game_days = 0.5 / 24.0;
    assert!(game.world_mut().drive_actor(actor));
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(FIRST)));
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "begins"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "ends"),
        Some(Value::Int(0))
    );
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "changes"),
        Some(Value::Int(0))
    );
    let actor_object = game.object_of_form(actor);
    assert_eq!(
        game.vm
            .get_var(package_object, "PackFragments", "beginActor"),
        Some(Value::Object(actor_object))
    );
    game.world_mut().actor(actor).unwrap().ai.reevaluate = true;
    game.tick(0.1);
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "begins"),
        Some(Value::Int(1))
    );

    game.world_mut().clock.game_days = 1.5 / 24.0;
    game.world_mut().actor(actor).unwrap().ai.reevaluate = true;
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(SECOND)));
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "begins"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "changes"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "begins"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "beginActor"),
        Some(Value::Object(actor_object))
    );
    assert_eq!(
        game.vm
            .get_var(package_object, "PackFragments", "changeActor"),
        Some(Value::Object(actor_object))
    );
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "ends"),
        Some(Value::Int(0))
    );
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "changes"),
        Some(Value::Int(0))
    );

    game.world_mut().clock.game_days = 3.0 / 24.0;
    game.world_mut().actor(actor).unwrap().ai.reevaluate = true;
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), None);
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "begins"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "changes"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "ends"),
        Some(Value::Int(0))
    );
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "ends"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package_object, "PackFragments", "endActor"),
        Some(Value::None)
    );
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "endActor"),
        Some(Value::Object(actor_object))
    );

    game.world_mut().actor(actor).unwrap().ai.reevaluate = true;
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), None);
    assert_eq!(
        game.vm
            .get_var(second_package_object, "PackFragments", "ends"),
        Some(Value::Int(1))
    );
}
