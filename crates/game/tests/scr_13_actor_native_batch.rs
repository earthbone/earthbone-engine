// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-13 Actor native behavior and explicit unsupported boundaries.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{
    plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::{npc, Game};
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TRACK_CRIME: u32 = 0x40;
static PROBES: AtomicUsize = AtomicUsize::new(0);

fn actor(form: u32, edid: &str, crime_faction: Option<u32>) -> Record {
    let record = npc::build_record(
        form,
        edid,
        edid,
        npc::Config::default(),
        npc::Stats::default(),
    );
    match crime_faction {
        Some(faction) => record.sub(T(b"CRIF"), faction.to_le_bytes()),
        None => record,
    }
}

fn reference(form: u32, edid: &str, base: u32) -> Record {
    Record::new(T(b"ACHR"), form)
        .zsub(T(b"EDID"), edid)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), vec![0; 24])
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    let fact = Record::new(T(b"FACT"), 0x701)
        .zsub(T(b"EDID"), "CrimeFaction")
        .sub(T(b"DATA"), TRACK_CRIME.to_le_bytes())
        .sub(T(b"CRVA"), [0u8, 0, 0, 0, 0, 0, 75, 0, 0, 0, 0, 0]);
    plugin.push_top(fact);
    plugin.push_top(actor(0x702, "GuardBase", None));
    plugin.push_top(actor(0x703, "CriminalBase", Some(0x701)));
    let cell = plugin.push_record(
        Record::new(T(b"CELL"), 0x704)
            .zsub(T(b"EDID"), "Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(0x704));
    for record in [
        reference(0x705, "GuardRef", 0x702),
        reference(0x706, "CriminalRef", 0x703),
    ] {
        let index = plugin.push_record(record);
        temporary.entries.push(Entry::Record(index));
    }
    let mut children = Group::new(GroupKind::CellChildren(0x704));
    children.entries.push(Entry::Group(temporary));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    plugin
}

pub fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("SCR13.esm", Plugin::parse(&plugin().to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Object(_) | Value::None => "Form",
        _ => "Var",
    }
}

pub fn call(game: &mut Game, target: Value, name: &str, args: &[Value]) -> Value {
    let index = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Scr13Probe{index}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for (index, value) in args.iter().enumerate() {
        let parameter = format!("arg{index}");
        function = function.param(&parameter, type_of(value));
        operands.push(ident(&parameter));
    }
    let bytes = ScriptBuilder::new(&script, "Form")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut arguments = vec![target];
    arguments.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &arguments).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

pub fn object(game: &mut Game, id: GlobalFormId) -> Value {
    Value::Object(game.object_of_form(id))
}

#[test]
fn scr_13_send_trespass_alarm_records_the_crime_and_rejects_invalid_actor() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    let guard_object = object(&mut game, guard);
    let criminal_object = object(&mut game, criminal);

    assert_eq!(
        call(
            &mut game,
            guard_object.clone(),
            "SendTrespassAlarm",
            std::slice::from_ref(&criminal_object),
        ),
        Value::None
    );
    assert_eq!(
        game.world_mut()
            .faction(GlobalFormId {
                file: 0,
                object: 0x701
            })
            .unwrap()
            .crime_gold_nonviolent,
        75
    );
    assert!(game
        .world_mut()
        .actor(guard)
        .unwrap()
        .witnessed_crimes
        .contains(&criminal));

    call(&mut game, guard_object, "SendTrespassAlarm", &[Value::None]);
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("akCriminal must be an Actor")));
    assert_eq!(
        game.world_mut()
            .faction(GlobalFormId {
                file: 0,
                object: 0x701
            })
            .unwrap()
            .crime_gold_nonviolent,
        75
    );
}

#[test]
fn scr_13_stop_combat_alarm_clears_target_combat_and_alert() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let criminal = game.form("CriminalRef").unwrap();
    {
        let mut world = game.world_mut();
        let actor = world.actor(guard).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(criminal);
        actor.alerted = true;
    }

    let guard_object = object(&mut game, guard);
    assert_eq!(
        call(&mut game, guard_object, "StopCombatAlarm", &[]),
        Value::None
    );
    let mut world = game.world_mut();
    let actor = world.actor(guard).unwrap();
    assert!(!actor.in_combat);
    assert_eq!(actor.combat_target, None);
    assert!(!actor.alerted);
    assert!(game.traces().is_empty());
}

#[test]
fn scr_13_unavailable_actor_systems_report_capabilities_instead_of_succeeding_quietly() {
    let mut game = game();
    let guard = game.form("GuardRef").unwrap();
    let target = object(&mut game, guard);
    for (name, reason) in [
        (
            "ShowBarterMenu",
            "barter transactions and menu are not implemented",
        ),
        (
            "OpenInventory",
            "actor inventory presenter is not implemented",
        ),
        (
            "ClearForcedMovement",
            "forced movement state is not implemented",
        ),
    ] {
        assert_eq!(call(&mut game, target.clone(), name, &[]), Value::None);
        assert!(
            game.traces().iter().any(|trace| trace.contains(reason)),
            "{name}"
        );
    }
}
