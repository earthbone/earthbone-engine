// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
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

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn pack_fragment(function: &str) -> PackFragment {
    PackFragment {
        marker: 1,
        script: "BoundaryFragments".into(),
        function: function.into(),
    }
}

fn package_record(form: u32, name: &str, hour: i8, fragments: Option<PackFragments>) -> Record {
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
    if let Some(pack_fragments) = fragments {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new("BoundaryFragments")],
            pack_fragments: Some(pack_fragments),
            ..Vmad::default()
        };
        record = record.sub(T(b"VMAD"), vmad.write(T(b"PACK")));
    }
    record
}

fn plugin(first: Option<PackFragments>, second: Option<PackFragments>) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "BoundaryActor")
            .sub(T(b"PKID"), FIRST.to_le_bytes())
            .sub(T(b"PKID"), SECOND.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "BoundaryActorRef")
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    plugin.push_top(package_record(FIRST, "FirstPackage", 0, first));
    plugin.push_top(package_record(SECOND, "SecondPackage", 1, second));
    plugin
}

fn fragment(script: &str, function: &str) -> PackFragment {
    PackFragment {
        marker: 1,
        script: script.into(),
        function: function.into(),
    }
}

fn fragments(
    begin: Option<PackFragment>,
    end: Option<PackFragment>,
    change: Option<PackFragment>,
) -> PackFragments {
    PackFragments {
        version: 2,
        file: "BoundaryFragments".into(),
        begin,
        end,
        change,
    }
}

fn script() -> Vec<u8> {
    let callback = |function: &str, counter: &str, actor_var: &str| {
        FunctionBuilder::new(function, "None")
            .param("akActor", "Actor")
            .ins(IAdd, vec![ident(counter), ident(counter), i(1)])
            .ins(Assign, vec![ident(actor_var), ident("akActor")])
            .build()
    };
    ScriptBuilder::new("BoundaryFragments", "Package")
        .variable("begins", "Int", i(0))
        .variable("ends", "Int", i(0))
        .variable("changes", "Int", i(0))
        .variable("beginActor", "Actor", none())
        .variable("endActor", "Actor", none())
        .variable("changeActor", "Actor", none())
        .function(callback("Begin", "begins", "beginActor"))
        .function(callback("End", "ends", "endActor"))
        .function(callback("Change", "changes", "changeActor"))
        .bytes()
}

fn game(first: Option<PackFragments>, second: Option<PackFragments>) -> Game {
    let mut order = LoadOrder::new();
    order.add("Ai04Breaker.esm", plugin(first, second)).unwrap();
    let mut game = Game::new(order);
    game.load_script(&script()).unwrap();
    game.bind();
    game
}

fn reevaluate(game: &mut Game, at_hour: f64) {
    game.world_mut().clock.game_days = at_hour / 24.0;
    game.world_mut().actor(id(ACTOR)).unwrap().ai.reevaluate = true;
    game.tick(0.001);
}

#[test]
fn ai_04_begin_fragment_runs_once_when_the_same_package_is_reevaluated() {
    let first = fragments(Some(pack_fragment("Begin")), None, None);
    let mut game = game(Some(first), None);
    let package = game.object_of_form(id(FIRST));
    let actor = game.object_of_form(id(ACTOR));

    reevaluate(&mut game, 0.5);
    reevaluate(&mut game, 0.5);
    reevaluate(&mut game, 0.5);

    assert_eq!(game.world().current_package(id(ACTOR)), Some(id(FIRST)));
    assert_eq!(
        game.vm.get_var(package, "BoundaryFragments", "begins"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package, "BoundaryFragments", "beginActor"),
        Some(Value::Object(actor))
    );
}

#[test]
fn ai_04_change_runs_on_departing_package_with_actor_once() {
    let first = fragments(
        Some(pack_fragment("Begin")),
        None,
        Some(pack_fragment("Change")),
    );
    let second = fragments(
        Some(pack_fragment("Begin")),
        None,
        Some(pack_fragment("Change")),
    );
    let mut game = game(Some(first), Some(second));
    let old_object = game.object_of_form(id(FIRST));
    let new_object = game.object_of_form(id(SECOND));
    let actor = game.object_of_form(id(ACTOR));

    reevaluate(&mut game, 0.5);
    reevaluate(&mut game, 1.5);
    reevaluate(&mut game, 1.5);

    assert_eq!(game.world().current_package(id(ACTOR)), Some(id(SECOND)));
    assert_eq!(
        game.vm.get_var(old_object, "BoundaryFragments", "changes"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm
            .get_var(old_object, "BoundaryFragments", "changeActor"),
        Some(Value::Object(actor))
    );
    assert_eq!(
        game.vm.get_var(new_object, "BoundaryFragments", "changes"),
        Some(Value::Int(0))
    );
}

#[test]
fn ai_04_missing_fragment_function_does_not_cancel_package_change() {
    let first = fragments(None, None, Some(fragment("AbsentScript", "Missing")));
    let mut game = game(Some(first), None);

    reevaluate(&mut game, 0.5);
    reevaluate(&mut game, 2.0);

    assert_eq!(game.world().current_package(id(ACTOR)), None);
}

#[test]
fn ai_04_end_fragment_runs_once_when_schedule_selects_no_replacement() {
    let first = fragments(
        Some(pack_fragment("Begin")),
        Some(pack_fragment("End")),
        None,
    );
    let mut game = game(Some(first), None);
    let package = game.object_of_form(id(FIRST));
    let actor = game.object_of_form(id(ACTOR));

    reevaluate(&mut game, 0.5);
    reevaluate(&mut game, 2.0);
    reevaluate(&mut game, 2.0);

    assert_eq!(game.world().current_package(id(ACTOR)), None);
    assert_eq!(
        game.vm.get_var(package, "BoundaryFragments", "ends"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(package, "BoundaryFragments", "endActor"),
        Some(Value::Object(actor))
    );
}

#[test]
fn ai_04_installed_pack_fragment_survey() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        eprintln!("SKYRIM_DATA not set, skipping installed PACK fragment survey");
        return;
    };

    let mut packs = 0;
    let mut vmads = 0;
    let mut fragments = [0; 3];
    for (id, record) in order
        .winners_of(Tag(*b"PACK"))
        .filter(|(_, record)| !record.is_deleted())
    {
        packs += 1;
        let Some(raw) = record.find(Tag(*b"VMAD")) else {
            continue;
        };
        let parsed = Vmad::parse(raw, Tag(*b"PACK"))
            .unwrap_or_else(|error| panic!("{id}: invalid PACK VMAD: {error}"));
        vmads += 1;
        if let Some(pack) = parsed.pack_fragments {
            fragments[0] += usize::from(pack.begin.is_some());
            fragments[1] += usize::from(pack.end.is_some());
            fragments[2] += usize::from(pack.change.is_some());
        }
    }

    // Thresholds are fixed before the installed-data survey.
    assert!(
        packs > 100,
        "expected installed PACK records, found {packs}"
    );
    assert!(
        vmads > 0,
        "installed PACK records contain no VMAD subrecords"
    );
    assert!(
        fragments.iter().sum::<usize>() > 0,
        "no installed PACK fragments found"
    );
    eprintln!(
        "installed PACK survey: {packs} PACK records, {vmads} VMADs, fragments {fragments:?}"
    );
}
