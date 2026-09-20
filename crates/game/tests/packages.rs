// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Packages drive an actor: a synthetic plugin with two navmeshed cells, a
//! guard whose package list is a conditional travel, a scheduled sleep and
//! a sandbox, and a quest whose alias package outranks them all. The gated
//! test at the end reads every `PACK` of the installed `Skyrim.esm` and
//! evaluates the Prisoner's stack.

use soe_esm::records::navm::{EdgeLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::{self, Procedure};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Location, Schedule};
use soe_game::quest;
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_navmesh::vanilla::flags as nav_flags;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const RACE: u32 = 0x700;
const BED_KEYWORD: u32 = 0x701;
const BED_BASE: u32 = 0x702;
const IDLE_BASE: u32 = 0x703;
const MARKER_BASE: u32 = 0x704;
const TRAVEL_TEMPLATE: u32 = 0x710;
const SANDBOX_TEMPLATE: u32 = 0x711;
const SLEEP_TEMPLATE: u32 = 0x712;
const TRAVEL: u32 = 0x720;
const SLEEP: u32 = 0x721;
const SANDBOX: u32 = 0x722;
const ALIAS_TRAVEL: u32 = 0x723;
const GUARD_QUEST: u32 = 0x730;
const ALIAS_QUEST: u32 = 0x731;
const GUARD_BASE: u32 = 0x740;
const CELL_A: u32 = 0x800;
const CELL_B: u32 = 0x801;
const MARKER_B: u32 = 0x805;
const BEDROLL: u32 = 0x806;
const GUARD: u32 = 0x807;
const MARKER_A: u32 = 0x808;
const NAVM_A: u32 = 0x810;
const NAVM_B: u32 = 0x811;
const IDLE_A: u32 = 0x822;

fn placed(tag: [u8; 4], id: u32, edid: &str, base: u32, at: [f32; 3]) -> Record {
    let mut data = Vec::with_capacity(24);
    for v in at {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&[0u8; 12]);
    Record::new(T(&tag), id)
        .zsub(EDID, edid)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

/// A 100 by 100 square of two triangles at `origin` on the x axis. The
/// right edge of the first triangle and the left edge of the second link
/// out when asked, the way two adjacent cells' navmeshes link.
fn square(cell: u32, origin: f32, right: Option<(u32, i16)>, left: Option<(u32, i16)>) -> Geometry {
    let o = origin;
    let mut t0 = Triangle {
        vertices: [0, 1, 2],
        edges: [-1, -1, 1],
        flags: 0,
        cover_flags: 0,
    };
    let mut t1 = Triangle {
        vertices: [0, 2, 3],
        edges: [0, -1, -1],
        flags: 0,
        cover_flags: 0,
    };
    let mut edge_links = Vec::new();
    if let Some((navmesh, triangle)) = right {
        t0.edges[1] = edge_links.len() as i16;
        t0.flags |= nav_flags::EDGE_1_2_LINK;
        edge_links.push(EdgeLink {
            kind: 2,
            navmesh,
            triangle,
        });
    }
    if let Some((navmesh, triangle)) = left {
        t1.edges[2] = edge_links.len() as i16;
        t1.flags |= nav_flags::EDGE_2_0_LINK;
        edge_links.push(EdgeLink {
            kind: 2,
            navmesh,
            triangle,
        });
    }
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(cell),
        vertices: vec![
            [o, 0.0, 0.0],
            [o + 100.0, 0.0, 0.0],
            [o + 100.0, 100.0, 0.0],
            [o, 100.0, 0.0],
        ],
        triangles: vec![t0, t1],
        edge_links,
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [100.0, 100.0],
            min: [o, 0.0, 0.0],
            max: [o + 100.0, 100.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

fn interior_cell(p: &mut Plugin, id: u32, edid: &str, records: Vec<Record>) -> Group {
    let cell = p.push_record(
        Record::new(T(b"CELL"), id)
            .zsub(EDID, edid)
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut temp = Group::new(GroupKind::CellTemporary(id));
    for r in records {
        let idx = p.push_record(r);
        temp.entries.push(Entry::Record(idx));
    }
    let mut children = Group::new(GroupKind::CellChildren(id));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    sub
}

fn template(id: u32, edid: &str, procedure: &str, input: &str) -> Record {
    let rec = package::build_template(id, edid);
    let rec = package::close_header(rec, None);
    let rec = package::with_location_input(rec, 0, Location::NearSelf, 0);
    let rec = package::finish(rec);
    let rec = package::with_procedure(rec, procedure, &[0], true);
    let rec = package::with_input_name(rec, 0, input, true);
    package::with_actions(rec)
}

fn travel_to(id: u32, edid: &str, target: u32, conditions: &[Condition]) -> Record {
    let mut rec = package::build_record(id, edid, 0, Schedule::default());
    for c in conditions {
        rec = package::with_condition(rec, c);
    }
    let rec = package::close_header(rec, Some(TRAVEL_TEMPLATE));
    let rec = package::with_location_input(rec, 0, Location::NearReference(target), 0);
    package::with_actions(package::finish(rec))
}

fn test_plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    p.push_top(Record::new(T(b"RACE"), RACE).zsub(EDID, "NordRace"));
    p.push_top(Record::new(T(b"KYWD"), BED_KEYWORD).zsub(EDID, "FurnitureBedRoll"));
    p.push_top(
        Record::new(T(b"FURN"), BED_BASE)
            .zsub(EDID, "BedRollBase")
            .sub(T(b"KSIZ"), 1u32.to_le_bytes())
            .sub(T(b"KWDA"), BED_KEYWORD.to_le_bytes()),
    );
    p.push_top(Record::new(T(b"IDLM"), IDLE_BASE).zsub(EDID, "IdleMarkerBase"));
    p.push_top(Record::new(T(b"STAT"), MARKER_BASE).zsub(EDID, "MarkerBase"));

    p.push_top(template(
        TRAVEL_TEMPLATE,
        "TravelTemplate",
        "Travel",
        "Place to Travel",
    ));
    p.push_top(template(
        SANDBOX_TEMPLATE,
        "SandboxTemplate",
        "Sandbox",
        "Location",
    ));
    p.push_top(template(
        SLEEP_TEMPLATE,
        "SleepTemplate",
        "Sleep",
        "Sleep Location",
    ));

    let stage_reached = Condition {
        comparison: condition::GREATER_OR_EQUAL,
        flags: 0,
        value: CompValue::Float(10.0),
        function: 58,
        param1: GUARD_QUEST,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    p.push_top(travel_to(TRAVEL, "GuardTravel", MARKER_B, &[stage_reached]));
    p.push_top(travel_to(ALIAS_TRAVEL, "AliasTravel", MARKER_A, &[]));
    let sleep = package::build_record(
        SLEEP,
        "GuardSleep",
        0,
        Schedule {
            hour: 22,
            duration_minutes: 480,
            ..Default::default()
        },
    );
    let sleep = package::close_header(sleep, Some(SLEEP_TEMPLATE));
    let sleep = package::with_location_input(sleep, 0, Location::NearReference(BEDROLL), 0);
    p.push_top(package::with_actions(package::finish(sleep)));
    let sandbox = package::build_record(SANDBOX, "GuardSandbox", 0, Schedule::default());
    let sandbox = package::close_header(sandbox, Some(SANDBOX_TEMPLATE));
    let sandbox = package::with_location_input(sandbox, 0, Location::NearSelf, 200);
    p.push_top(package::with_actions(package::finish(sandbox)));

    let q = quest::build_record(GUARD_QUEST, "GuardQuest", 0);
    let q = quest::with_stage(q, 0, quest::stage_flags::START_UP, 0, "");
    let q = quest::with_stage(q, 10, 0, 0, "Go");
    p.push_top(q);
    let aq = quest::build_record(ALIAS_QUEST, "AliasQuest", 0);
    let aq = quest::with_stage(aq, 0, quest::stage_flags::START_UP, 0, "");
    let aq = quest::with_alias_packages(aq, 0, "Guard", Some(GUARD), &[ALIAS_TRAVEL]);
    p.push_top(aq);

    let guard = npc::build_record(
        GUARD_BASE,
        "GuardBase",
        "Guard",
        Config {
            level: 5,
            ..Default::default()
        },
        Stats {
            health: 100,
            ..Default::default()
        },
    )
    .sub(T(b"RNAM"), RACE.to_le_bytes())
    .sub(T(b"PKID"), TRAVEL.to_le_bytes())
    .sub(T(b"PKID"), SLEEP.to_le_bytes())
    .sub(T(b"PKID"), SANDBOX.to_le_bytes());
    p.push_top(guard);

    let guard_script = Vmad {
        scripts: vec![ScriptEntry::new("GuardScript")],
        ..Default::default()
    }
    .write(T(b"ACHR"));
    let cell_a = interior_cell(
        &mut p,
        CELL_A,
        "CellA",
        vec![
            placed(*b"ACHR", GUARD, "GuardRef", GUARD_BASE, [10.0, 50.0, 0.0])
                .sub(T(b"VMAD"), guard_script),
            placed(
                *b"REFR",
                MARKER_A,
                "MarkerA",
                MARKER_BASE,
                [90.0, 90.0, 0.0],
            ),
            placed(*b"REFR", IDLE_A, "IdleA", IDLE_BASE, [50.0, 50.0, 0.0]),
            Record::new(T(b"NAVM"), NAVM_A).sub(
                T(b"NVNM"),
                square(CELL_A, 0.0, Some((NAVM_B, 1)), None).to_bytes(),
            ),
        ],
    );
    let cell_b = interior_cell(
        &mut p,
        CELL_B,
        "CellB",
        vec![
            placed(
                *b"REFR",
                MARKER_B,
                "MarkerB",
                MARKER_BASE,
                [190.0, 50.0, 0.0],
            ),
            placed(*b"REFR", BEDROLL, "BedRoll", BED_BASE, [150.0, 20.0, 0.0]),
            Record::new(T(b"NAVM"), NAVM_B).sub(
                T(b"NVNM"),
                square(CELL_B, 100.0, None, Some((NAVM_A, 0))).to_bytes(),
            ),
        ],
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(cell_a));
    block.entries.push(Entry::Group(cell_b));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

fn guard_script() -> Vec<u8> {
    ScriptBuilder::new("GuardScript", "Actor")
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("akActionRef", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![s("EvaluatePackage"), ident("self"), ident("::nonevar")],
                )
                .build(),
        )
        .bytes()
}

pub fn game_at_hour(hour: f64) -> Game {
    let bytes = test_plugin().to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Packages.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    game.load_script(&guard_script()).unwrap();
    game.world_mut().clock.game_days = hour / 24.0;
    let guard = game.form("GuardRef").unwrap();
    assert!(game.world_mut().drive_actor(guard));
    game
}

pub fn tick(game: &mut Game, seconds: f64) {
    let steps = (seconds / 0.1).round() as usize;
    for _ in 0..steps {
        game.tick(0.1);
    }
}

fn package_name(game: &Game, actor: soe_esm::GlobalFormId) -> String {
    let w = game.world();
    w.current_package(actor)
        .and_then(|p| w.editor_id(p))
        .unwrap_or_default()
}

#[test]
fn the_first_package_whose_conditions_hold_wins() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardSandbox");
    assert_eq!(
        game.world()
            .actor_ref(guard)
            .map(|a| a.ai.procedure.clone()),
        Some(Procedure::Sandbox)
    );
    let quest = game.form("GuardQuest").unwrap();
    assert!(game.set_stage(quest, 10));
    tick(&mut game, 1.1);
    assert_eq!(
        package_name(&game, guard),
        "GuardTravel",
        "the stack is re-evaluated once a second"
    );
    assert_eq!(game.world().ai.unsupported_functions.len(), 0);
}

#[test]
fn a_travel_package_walks_the_navmesh_into_the_next_cell() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("GuardQuest").unwrap();
    let cell_a = game.form("CellA").unwrap();
    let cell_b = game.form("CellB").unwrap();
    game.set_stage(quest, 10);
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardTravel");
    {
        let mut w = game.world_mut();
        let r = w.reference(guard).unwrap();
        assert_eq!(r.cell, Some(cell_a));
        assert!(r.position[0] < 30.0, "just started at {:?}", r.position);
    }
    tick(&mut game, 0.5);
    let mid = game.world_mut().reference(guard).unwrap().position;
    assert!(mid[0] > 30.0 && mid[0] < 190.0, "walking at {mid:?}");
    tick(&mut game, 3.0);
    let mut w = game.world_mut();
    let r = w.reference(guard).unwrap();
    assert!(
        (r.position[0] - 190.0).abs() < 5.0 && (r.position[1] - 50.0).abs() < 5.0,
        "arrived at {:?}",
        r.position
    );
    assert_eq!(
        r.cell,
        Some(cell_b),
        "crossing the navmesh link changed cells"
    );
    let travel = w.form("GuardTravel").unwrap();
    let a = w.actor(guard).unwrap();
    assert!(a.ai.arrived);
    assert!(a.ai.completed.contains_key(&travel));
}

#[test]
fn a_sandbox_walks_to_an_idle_marker_and_dwells() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    tick(&mut game, 1.0);
    let mut w = game.world_mut();
    let pos = w.reference(guard).unwrap().position;
    assert!(
        (pos[0] - 50.0).abs() < 5.0 && (pos[1] - 50.0).abs() < 5.0,
        "at the idle marker, not {pos:?}"
    );
    let a = w.actor(guard).unwrap();
    assert!(a.ai.arrived);
    assert!(a.ai.dwell_until > w.clock.game_hours());
}

#[test]
fn a_sleep_package_runs_on_schedule_and_holds_the_bed() {
    let mut game = game_at_hour(23.0);
    let guard = game.form("GuardRef").unwrap();
    let cell_b = game.form("CellB").unwrap();
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardSleep");
    assert_eq!(
        game.world().actor_ref(guard).and_then(|a| a.ai.bed),
        game.form("BedRoll")
    );
    tick(&mut game, 3.0);
    {
        let mut w = game.world_mut();
        let r = w.reference(guard).unwrap();
        assert!(
            (r.position[0] - 150.0).abs() < 5.0,
            "at the bed, not {:?}",
            r.position
        );
        assert_eq!(r.cell, Some(cell_b));
        assert!(w.actor(guard).unwrap().ai.sleeping);
    }
    game.world_mut().clock.game_days += 9.0 / 24.0;
    tick(&mut game, 1.1);
    assert_eq!(
        package_name(&game, guard),
        "GuardSandbox",
        "08:00 ends the sleep window"
    );
    assert!(!game.world().actor_ref(guard).unwrap().ai.sleeping);
}

#[test]
fn an_alias_package_outranks_the_base_list() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("AliasQuest").unwrap();
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardSandbox");
    assert!(game.start_quest(quest));
    tick(&mut game, 1.1);
    assert_eq!(package_name(&game, guard), "AliasTravel");
    assert_eq!(
        game.world().actor_ref(guard).and_then(|a| a.ai.alias),
        Some((quest, 0))
    );
    tick(&mut game, 2.0);
    let pos = game.world_mut().reference(guard).unwrap().position;
    assert!(
        (pos[0] - 90.0).abs() < 5.0 && (pos[1] - 90.0).abs() < 5.0,
        "at MarkerA, not {pos:?}"
    );
}

#[test]
fn evaluate_package_from_a_script_switches_at_once() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("GuardQuest").unwrap();
    game.bind();
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardSandbox");
    game.set_stage(quest, 10);
    let player = game.player();
    assert!(game.activate(guard, player));
    assert_eq!(
        package_name(&game, guard),
        "GuardTravel",
        "the native re-evaluated without waiting for the interval"
    );
}

#[test]
fn the_driver_state_survives_a_save() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("GuardQuest").unwrap();
    game.set_stage(quest, 10);
    tick(&mut game, 0.5);
    let bytes = game.save_state().unwrap();
    let before = game.world_mut().reference(guard).unwrap().position;

    let plugin = test_plugin().to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Packages.esm", Plugin::parse(&plugin).unwrap())
        .unwrap();
    let mut restored = Game::new(order);
    restored.load_script(&guard_script()).unwrap();
    restored.load_state(&bytes).unwrap();
    assert_eq!(package_name(&restored, guard), "GuardTravel");
    assert_eq!(
        restored
            .world()
            .actor_ref(guard)
            .map(|a| a.ai.procedure.clone()),
        Some(Procedure::Travel)
    );
    assert_eq!(
        restored.world_mut().reference(guard).unwrap().position,
        before
    );
    tick(&mut restored, 3.0);
    let pos = restored.world_mut().reference(guard).unwrap().position;
    assert!(
        (pos[0] - 190.0).abs() < 5.0,
        "kept walking after the load, at {pos:?}"
    );
}

#[test]
fn the_installed_game_reads_every_package_and_evaluates_the_prisoner() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let started = std::time::Instant::now();
    let bytes = std::fs::read(format!("{data}/Skyrim.esm")).unwrap();
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let game = Game::new(order);

    let mut packages = 0usize;
    let mut templates = 0usize;
    let mut failed = Vec::new();
    let mut with_conditions = 0usize;
    let mut with_schedule = 0usize;
    let mut procedures: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::default();
    let ids: Vec<_> = game
        .world()
        .order()
        .winners_of(T(b"PACK"))
        .map(|(id, _)| id)
        .collect();
    for id in &ids {
        let w = game.world();
        let rec = w.record(*id).unwrap();
        match soe_game::package::Package::from_record(rec) {
            Ok(p) => {
                packages += 1;
                if p.is_template() {
                    templates += 1;
                    for b in p.procedures() {
                        *procedures
                            .entry(b.procedure.clone().unwrap_or_default())
                            .or_default() += 1;
                    }
                }
                if !p.conditions.is_empty() {
                    with_conditions += 1;
                }
                if p.schedule.hour >= 0 {
                    with_schedule += 1;
                }
            }
            Err(e) => failed.push(format!("{id}: {e}")),
        }
    }
    eprintln!(
        "Skyrim.esm: {packages} packages read ({templates} templates), {} failed, {with_conditions} with conditions, {with_schedule} scheduled, in {:.2}s",
        failed.len(),
        started.elapsed().as_secs_f32()
    );
    for f in failed.iter().take(10) {
        eprintln!("  {f}");
    }
    eprintln!("procedure names in the templates:");
    for (name, n) in &procedures {
        eprintln!("  {name}: {n}");
    }
    assert!(failed.is_empty(), "every PACK parses");

    let mut supported = 0usize;
    let mut kinds: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::default();
    for id in &ids {
        let (procedure, location, _) = ai::procedure_of(&game.world(), *id);
        let name = match &procedure {
            Procedure::None => "none".to_string(),
            Procedure::Travel => "Travel".to_string(),
            Procedure::Sandbox => "Sandbox".to_string(),
            Procedure::Sleep => "Sleep".to_string(),
            Procedure::Wait { .. } => "Wait".to_string(),
            Procedure::Unsupported(n) => format!("unsupported {n}"),
        };
        if matches!(
            procedure,
            Procedure::Travel | Procedure::Sandbox | Procedure::Sleep | Procedure::Wait { .. }
        ) {
            supported += 1;
        }
        let _ = location;
        *kinds.entry(name).or_default() += 1;
    }
    eprintln!(
        "{supported} of {} packages resolve to a procedure the driver runs:",
        ids.len()
    );
    for (name, n) in &kinds {
        eprintln!("  {name}: {n}");
    }

    // The Prisoner base ships without a package list, so the innkeeper of
    // the Bannered Mare stands in as the actor with a real stack.
    let world = game.shared_world();
    let mut stacks = Vec::new();
    for edid in ["Prisoner", "Hulda"] {
        let base = game
            .form(edid)
            .unwrap_or_else(|| panic!("{edid} in Skyrim.esm"));
        let actor = {
            let mut w = world.borrow_mut();
            let at = w.player();
            w.place_at(base, at, false)
        };
        let names: Vec<String> = {
            let mut w = world.borrow_mut();
            let candidates = ai::candidates(&mut w, actor);
            candidates
                .iter()
                .map(|c| {
                    w.editor_id(c.package)
                        .unwrap_or_else(|| format!("{}", c.package))
                })
                .collect()
        };
        eprintln!("{edid} package stack ({}): {names:?}", names.len());
        for hour in [0.0, 9.0, 14.0, 20.0] {
            let chosen = {
                let mut w = world.borrow_mut();
                w.clock.game_days = hour / 24.0;
                ai::select(&mut w, actor)
            };
            let (name, procedure) = match chosen {
                Some(c) => (
                    game.world().editor_id(c.package).unwrap_or_default(),
                    ai::procedure_of(&game.world(), c.package).0,
                ),
                None => (String::from("none"), Procedure::None),
            };
            eprintln!("  {edid} at {hour:02.0}:00 runs {name} ({procedure:?})");
        }
        stacks.push(names.len());
    }
    let unsupported: Vec<_> = game
        .world()
        .ai
        .unsupported_functions
        .iter()
        .map(|(f, n)| format!("{f}:{n}"))
        .collect();
    eprintln!("condition functions without an implementation: {unsupported:?}");
    assert_eq!(stacks[0], 0, "the Prisoner base has no PKID list");
    assert!(stacks[1] > 0, "Hulda has packages");
    eprintln!("total {:.2}s", started.elapsed().as_secs_f32());
}
