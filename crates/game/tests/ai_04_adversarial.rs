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

fn package(form: u32, name: &str, hour: i8, on_change: bool) -> Record {
    let mut record = package::build_record(
        form,
        name,
        0,
        Schedule {
            hour,
            duration_minutes: 60,
            ..Schedule::default()
        },
    );
    if on_change {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("PackFragments")],
            pack_fragments: Some(PackFragments {
                version: 2,
                file: "PackFragments".into(),
                begin: None,
                end: None,
                change: Some(PackFragment {
                    marker: 1,
                    script: "PackFragments".into(),
                    function: "OnChange".into(),
                }),
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
    plugin.push_top(package(FIRST, "FirstPackage", 0, true));
    plugin.push_top(package(SECOND, "SecondPackage", 1, false));
    plugin
}

fn script() -> Vec<u8> {
    let callback = FunctionBuilder::new("OnChange", "None")
        .param("akActor", "Actor")
        .ins(IAdd, vec![ident("changes"), ident("changes"), i(1)])
        .ins(Assign, vec![ident("changeActor"), ident("akActor")])
        .build();
    ScriptBuilder::new("PackFragments", "Package")
        .variable("changes", "Int", i(0))
        .variable("changeActor", "Actor", none())
        .function(callback)
        .bytes()
}

#[test]
fn ai_04_sparse_change_fragment_runs_once_on_the_replaced_package() {
    let mut order = LoadOrder::new();
    order.add("Ai04.esm", plugin()).unwrap();
    let mut game = Game::new(order);
    game.load_script(&script()).unwrap();
    game.bind();

    let actor = id(ACTOR);
    let first_package = game.object_of_form(id(FIRST));
    let actor_object = game.object_of_form(actor);
    game.world_mut().clock.game_days = 0.5 / 24.0;
    assert!(game.world_mut().drive_actor(actor));
    game.tick(0.1);
    assert_eq!(game.world().current_package(actor), Some(id(FIRST)));

    game.world_mut().clock.game_days = 1.5 / 24.0;
    game.world_mut().actor(actor).unwrap().ai.reevaluate = true;
    game.tick(0.1);

    assert_eq!(game.world().current_package(actor), Some(id(SECOND)));
    assert_eq!(
        game.vm.get_var(first_package, "PackFragments", "changes"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm
            .get_var(first_package, "PackFragments", "changeActor"),
        Some(Value::Object(actor_object))
    );
}
