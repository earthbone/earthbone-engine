// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Packages drive an actor: a synthetic plugin with two navmeshed cells, a
//! guard whose package list is a conditional travel, a scheduled sleep and
//! a sandbox, and a quest whose alias package outranks them all. The gated
//! test at the end reads every `PACK` of the installed `Skyrim.esm` and
//! evaluates the Prisoner's stack.

use soe_esm::records::navm::{EdgeLink, Geometry, NavGrid, Parent, Triangle};
use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::{self, AiState, Procedure};
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, condition, CompValue, Condition, Location, LocationData, Schedule};
use soe_game::quest;
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_navmesh::vanilla::flags as nav_flags;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const RACE: u32 = 0x700;
const WALK_MOVEMENT: u32 = 0x705;
const RUN_MOVEMENT: u32 = 0x706;
const INVALID_MOVEMENT: u32 = 0x707;
const DELETED_MOVEMENT: u32 = 0x708;
const BED_KEYWORD: u32 = 0x701;
const BED_BASE: u32 = 0x702;
const IDLE_BASE: u32 = 0x703;
const MARKER_BASE: u32 = 0x704;
const TRAVEL_TEMPLATE: u32 = 0x710;
const SANDBOX_TEMPLATE: u32 = 0x711;
const SLEEP_TEMPLATE: u32 = 0x712;
const IDLE_MARKER_TEMPLATE: u32 = 0x713;
const TRAVEL: u32 = 0x720;
const SLEEP: u32 = 0x721;
const SANDBOX: u32 = 0x722;
const ALIAS_TRAVEL: u32 = 0x723;
const LOCATION_ALIAS_TRAVEL: u32 = 0x724;
const RUN_SANDBOX: u32 = 0x725;
const USE_IDLE_MARKER: u32 = 0x726;
const GUARD_QUEST: u32 = 0x730;
const ALIAS_QUEST: u32 = 0x731;
const LOCATION_ALIAS_QUEST: u32 = 0x732;
const ALIAS_LOCATION: u32 = 0x733;
const OVERRIDE_LOCATION: u32 = 0x734;
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
const IDLE_TOO_FAR: u32 = 0x823;

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

fn movement_speeds(forward_walk: f32, forward_run: f32) -> Vec<u8> {
    let mut speeds = vec![0; 44];
    speeds[16..20].copy_from_slice(&forward_walk.to_le_bytes());
    speeds[20..24].copy_from_slice(&forward_run.to_le_bytes());
    speeds
}

fn with_preferred_speed(record: Record, speed: u8) -> Record {
    let mut data = record.find(T(b"PKDT")).unwrap().to_vec();
    let flags = u32::from_le_bytes(data[..4].try_into().unwrap()) | package::flags::PREFERRED_SPEED;
    data[..4].copy_from_slice(&flags.to_le_bytes());
    data[6] = speed;
    record.sub(T(b"PKDT"), data)
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

fn race_record(override_speeds: Option<(f32, f32)>) -> Record {
    race_record_with_walk(WALK_MOVEMENT, override_speeds)
}

fn race_record_with_walk(walk_movement: u32, override_speeds: Option<(f32, f32)>) -> Record {
    let race = Record::new(T(b"RACE"), RACE)
        .zsub(EDID, "NordRace")
        .sub(T(b"WKMV"), walk_movement.to_le_bytes())
        .sub(T(b"RNMV"), RUN_MOVEMENT.to_le_bytes());
    let Some((walk, run)) = override_speeds else {
        return race;
    };
    race.sub(T(b"MTYP"), walk_movement.to_le_bytes())
        .sub(T(b"SPED"), movement_speeds(walk, run))
}

fn test_plugin_for_race(race: Record) -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    p.push_top(
        Record::new(T(b"MOVT"), WALK_MOVEMENT)
            .zsub(EDID, "WalkMovement")
            .sub(T(b"SPED"), movement_speeds(110.0, 160.0)),
    );
    p.push_top(
        Record::new(T(b"MOVT"), RUN_MOVEMENT)
            .zsub(EDID, "RunMovement")
            .sub(T(b"SPED"), movement_speeds(180.0, 375.0)),
    );
    p.push_top(
        Record::new(T(b"STAT"), INVALID_MOVEMENT)
            .zsub(EDID, "NotMovement")
            .sub(T(b"SPED"), movement_speeds(999.0, 999.0)),
    );
    p.push_top(
        Record::new(T(b"MOVT"), DELETED_MOVEMENT)
            .zsub(EDID, "DeletedMovement")
            .sub(T(b"SPED"), movement_speeds(999.0, 999.0))
            .with_flags(flags::DELETED),
    );
    p.push_top(race);
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
    p.push_top(template(
        IDLE_MARKER_TEMPLATE,
        "IdleMarkerTemplate",
        "UseIdleMarker",
        "Location",
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
    let location_alias_travel = package::build_record(
        LOCATION_ALIAS_TRAVEL,
        "LocationAliasTravel",
        0,
        Schedule::default(),
    );
    let location_alias_travel = package::close_header(location_alias_travel, Some(TRAVEL_TEMPLATE));
    let location_alias_travel =
        package::with_location_input(location_alias_travel, 0, Location::AliasLocation(1), 0);
    p.push_top(package::with_actions(package::finish(
        location_alias_travel,
    )));
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
    let run_sandbox = package::build_record(
        RUN_SANDBOX,
        "GuardRunSandbox",
        0,
        Schedule {
            hour: 13,
            ..Default::default()
        },
    );
    let run_sandbox = package::close_header(run_sandbox, Some(SANDBOX_TEMPLATE));
    let run_sandbox = package::with_location_input(run_sandbox, 0, Location::NearSelf, 200);
    p.push_top(package::with_actions(package::finish(
        with_preferred_speed(run_sandbox, package::speed::RUN),
    )));
    let use_idle_marker = package::build_record(
        USE_IDLE_MARKER,
        "GuardUseIdleMarker",
        0,
        Schedule {
            hour: 5,
            duration_minutes: 60,
            ..Default::default()
        },
    );
    let use_idle_marker = package::close_header(use_idle_marker, Some(IDLE_MARKER_TEMPLATE));
    let use_idle_marker = package::with_location_input(use_idle_marker, 0, Location::NearSelf, 200);
    p.push_top(package::with_actions(package::finish(use_idle_marker)));

    let q = quest::build_record(GUARD_QUEST, "GuardQuest", 0);
    let q = quest::with_stage(q, 0, quest::stage_flags::START_UP, 0, "");
    let q = quest::with_stage(q, 10, 0, 0, "Go");
    p.push_top(q);
    let aq = quest::build_record(ALIAS_QUEST, "AliasQuest", 0);
    let aq = quest::with_stage(aq, 0, quest::stage_flags::START_UP, 0, "");
    let aq = quest::with_alias_packages(aq, 0, "Guard", Some(GUARD), &[ALIAS_TRAVEL]);
    p.push_top(aq);
    let lq = quest::build_record(LOCATION_ALIAS_QUEST, "LocationAliasQuest", 0);
    let lq = quest::with_stage(lq, 0, quest::stage_flags::START_UP, 0, "");
    let lq = quest::with_alias_packages(lq, 0, "Guard", Some(GUARD), &[LOCATION_ALIAS_TRAVEL]);
    let lq = lq
        .sub(T(b"ALLS"), 1_u32.to_le_bytes())
        .zsub(T(b"ALID"), "Destination")
        .sub(T(b"FNAM"), 0_u32.to_le_bytes())
        .sub(T(b"ALED"), []);
    p.push_top(lq);
    p.push_top(
        Record::new(T(b"LCTN"), ALIAS_LOCATION)
            .zsub(EDID, "AliasDestination")
            .sub(T(b"MNAM"), MARKER_A.to_le_bytes()),
    );

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
    .sub(T(b"PKID"), USE_IDLE_MARKER.to_le_bytes())
    .sub(T(b"PKID"), TRAVEL.to_le_bytes())
    .sub(T(b"PKID"), SLEEP.to_le_bytes())
    .sub(T(b"PKID"), RUN_SANDBOX.to_le_bytes())
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
            placed(
                *b"REFR",
                IDLE_TOO_FAR,
                "IdleTooFar",
                IDLE_BASE,
                [900.0, 900.0, 0.0],
            ),
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

fn test_plugin() -> Plugin {
    test_plugin_for_race(race_record(Some((123.0, 246.0))))
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
    game_at_hour_with_race(hour, race_record(Some((123.0, 246.0))))
}

fn game_at_hour_with_race(hour: f64, race: Record) -> Game {
    let bytes = test_plugin_for_race(race).to_bytes();
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
fn a_race_walks_at_its_record_override() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let race = game.form("NordRace").unwrap();
    let expected = {
        let world = game.world();
        let speeds = world.record(race).unwrap().find(T(b"SPED")).unwrap();
        f32::from_le_bytes(speeds[16..20].try_into().unwrap())
    };
    tick(&mut game, 0.1);
    assert_eq!(game.world().actor_ref(guard).unwrap().ai.speed, expected);
}

#[test]
fn a_race_without_override_uses_its_base_movement_type() {
    let mut game = game_at_hour_with_race(12.0, race_record(None));
    let guard = game.form("GuardRef").unwrap();
    let movement = game.form("WalkMovement").unwrap();
    let expected = {
        let world = game.world();
        let speeds = world.record(movement).unwrap().find(T(b"SPED")).unwrap();
        f32::from_le_bytes(speeds[16..20].try_into().unwrap())
    };
    tick(&mut game, 0.1);
    assert_eq!(game.world().actor_ref(guard).unwrap().ai.speed, expected);
}

#[test]
fn a_preferred_run_uses_the_race_run_movement_type() {
    let mut game = game_at_hour(13.0);
    let guard = game.form("GuardRef").unwrap();
    let movement = game.form("RunMovement").unwrap();
    let expected = {
        let world = game.world();
        let speeds = world.record(movement).unwrap().find(T(b"SPED")).unwrap();
        f32::from_le_bytes(speeds[20..24].try_into().unwrap())
    };
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardRunSandbox");
    assert_eq!(game.world().actor_ref(guard).unwrap().ai.speed, expected);
}

#[test]
fn invalid_or_deleted_movement_records_fall_back_to_the_default_walk_rate() {
    for race in [
        race_record_with_walk(INVALID_MOVEMENT, Some((999.0, 999.0))),
        race_record_with_walk(DELETED_MOVEMENT, Some((999.0, 999.0))),
    ] {
        let mut game = game_at_hour_with_race(12.0, race);
        let guard = game.form("GuardRef").unwrap();
        tick(&mut game, 0.1);
        assert_eq!(game.world().actor_ref(guard).unwrap().ai.speed, 110.0);
    }
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
fn ai_23_reserves_and_uses_an_eligible_idle_marker_then_releases_it_on_interruption() {
    let mut game = game_at_hour(5.0);
    let guard = game.form("GuardRef").unwrap();
    let marker = game.form("IdleA").unwrap();
    tick(&mut game, 0.1);
    assert_eq!(package_name(&game, guard), "GuardUseIdleMarker");
    assert_eq!(
        game.world().actor_ref(guard).unwrap().ai.procedure,
        Procedure::UseIdleMarker {
            marker: Some(marker)
        }
    );
    tick(&mut game, 1.0);
    assert!(game.world().actor_ref(guard).unwrap().ai.arrived);
    let (guard_position, marker_position) = {
        let mut world = game.world_mut();
        (
            world.reference(guard).unwrap().position,
            world.reference(marker).unwrap().position,
        )
    };
    assert_eq!(guard_position, marker_position);

    let saved = game.world().save_state();
    let mut old_version = saved.clone();
    old_version[4..6].copy_from_slice(&35_u16.to_le_bytes());
    assert!(game
        .world_mut()
        .load_state(&old_version)
        .unwrap_err()
        .to_string()
        .contains("UseIdleMarker procedure requires world version 36"));
    assert_eq!(game.world().save_state(), saved);
    game.world_mut().load_state(&saved).unwrap();
    assert_eq!(game.world().save_state(), saved);

    let quest = game.form("AliasQuest").unwrap();
    assert!(game.start_quest(quest));
    tick(&mut game, 1.1);
    assert_ne!(package_name(&game, guard), "GuardUseIdleMarker");
    assert_ne!(
        game.world().actor_ref(guard).unwrap().ai.procedure,
        Procedure::UseIdleMarker {
            marker: Some(marker)
        }
    );
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
fn an_alias_location_package_resolves_its_world_marker_destination() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("LocationAliasQuest").unwrap();
    assert!(game.start_quest(quest));
    let location = game.form("AliasDestination").unwrap();
    assert!(game.world_mut().force_alias(quest, 1, Some(location)));
    tick(&mut game, 1.1);
    assert_eq!(package_name(&game, guard), "LocationAliasTravel");
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
fn a_reference_alias_cannot_supply_a_location_package_destination() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("LocationAliasQuest").unwrap();
    let package = game.form("LocationAliasTravel").unwrap();
    let location = game.form("AliasDestination").unwrap();
    assert!(game.start_quest(quest));
    assert!(game.world_mut().force_alias(quest, 0, Some(location)));

    let destination = ai::resolve_location(
        &mut game.world_mut(),
        guard,
        package,
        Some((quest, 0)),
        &AiState::default(),
        &LocationData {
            location: Location::AliasLocation(0),
            radius: 0,
        },
    );
    assert_eq!(destination, None);
}

#[test]
fn a_location_marker_uses_the_winning_override_namespace() {
    let mut base = test_plugin();
    base.push_top(
        Record::new(T(b"LCTN"), OVERRIDE_LOCATION)
            .zsub(EDID, "OverrideDestination")
            .sub(T(b"MNAM"), MARKER_A.to_le_bytes()),
    );
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Packages.esm".into()],
        ..Default::default()
    });
    patch.push_top(
        Record::new(T(b"LCTN"), 0x0000_0734)
            .zsub(EDID, "OverrideDestination")
            .sub(T(b"MNAM"), 0x0100_0900_u32.to_le_bytes()),
    );
    patch.push_top(Record::new(T(b"REFR"), 0x0100_0900).zsub(EDID, "PatchMarker"));
    let mut order = LoadOrder::new();
    order.add("Packages.esm", base).unwrap();
    order.add("Override.esp", patch).unwrap();
    let game = Game::new(order);
    let location = game.form("OverrideDestination").unwrap();

    assert_eq!(
        game.world_mut().location_marker(location),
        Some(soe_esm::GlobalFormId {
            file: 1,
            object: 0x900,
        })
    );
}

#[test]
fn a_deleted_location_or_marker_has_no_package_destination() {
    let mut deleted_location = Record::new(T(b"LCTN"), 0x0000_0733)
        .zsub(EDID, "AliasDestination")
        .sub(T(b"MNAM"), MARKER_A.to_le_bytes());
    deleted_location.flags = flags::DELETED;
    let mut location_patch = Plugin::new(FileHeader {
        masters: vec!["Packages.esm".into()],
        ..Default::default()
    });
    location_patch.push_top(deleted_location);
    let mut location_order = LoadOrder::new();
    location_order.add("Packages.esm", test_plugin()).unwrap();
    location_order
        .add("DeletedLocation.esp", location_patch)
        .unwrap();
    let game = Game::new(location_order);
    let location = game.form("AliasDestination").unwrap();
    assert_eq!(game.world_mut().location_marker(location), None);

    let mut deleted_marker = Record::new(T(b"REFR"), 0x0000_0808).zsub(EDID, "MarkerA");
    deleted_marker.flags = flags::DELETED;
    let mut marker_patch = Plugin::new(FileHeader {
        masters: vec!["Packages.esm".into()],
        ..Default::default()
    });
    marker_patch.push_top(deleted_marker);
    let mut marker_order = LoadOrder::new();
    marker_order.add("Packages.esm", test_plugin()).unwrap();
    marker_order.add("DeletedMarker.esp", marker_patch).unwrap();
    let game = Game::new(marker_order);
    let location = game.form("AliasDestination").unwrap();
    assert_eq!(game.world_mut().location_marker(location), None);
}

#[test]
fn a_runtime_deleted_marker_has_no_package_destination() {
    let mut game = game_at_hour(12.0);
    let guard = game.form("GuardRef").unwrap();
    let quest = game.form("LocationAliasQuest").unwrap();
    let package = game.form("LocationAliasTravel").unwrap();
    let location = game.form("AliasDestination").unwrap();
    let marker = game.form("MarkerA").unwrap();
    assert!(game.start_quest(quest));
    let destination = {
        let mut world = game.world_mut();
        assert!(world.force_alias(quest, 1, Some(location)));
        world.reference(marker).unwrap().deleted = true;
        ai::resolve_location(
            &mut world,
            guard,
            package,
            Some((quest, 0)),
            &AiState::default(),
            &LocationData {
                location: Location::AliasLocation(1),
                radius: 0,
            },
        )
    };
    assert_eq!(destination, None);
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
            Procedure::Patrol => "Patrol".to_string(),
            Procedure::Wait { .. } => "Wait".to_string(),
            Procedure::Hover => "Hover".to_string(),
            Procedure::Follow => "Follow".to_string(),
            Procedure::ForceGreet => "ForceGreet".to_string(),
            Procedure::UseIdleMarker { .. } => "UseIdleMarker".to_string(),
            Procedure::Unsupported(n) => format!("unsupported {n}"),
        };
        if matches!(
            procedure,
            Procedure::Travel
                | Procedure::Sandbox
                | Procedure::Sleep
                | Procedure::Wait { .. }
                | Procedure::Patrol
                | Procedure::Hover
                | Procedure::Follow
                | Procedure::ForceGreet
                | Procedure::UseIdleMarker { .. }
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

/// FMT-03: PACK VMAD begin, end and change fragment entries must decode
/// from, and re-encode back to, the exact bytes the Creation Kit authored.
/// Loads Skyrim.esm, Update.esm and the three DLC masters and checks every
/// winning PACK record's VMAD subrecord.
#[test]
fn fmt_03_installed_pack_vmad_fragments_round_trip() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(format!("{data}/{name}")).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }

    let mut with_fragments = 0;
    let mut saw_begin = false;
    let mut saw_end = false;
    let mut saw_change = false;
    for (id, rec) in order.winners_of(T(b"PACK")) {
        let Some(bytes) = rec.find(T(b"VMAD")) else {
            continue;
        };
        let vmad = Vmad::parse(bytes, rec.tag).unwrap_or_else(|e| panic!("PACK {id:?} VMAD: {e}"));
        let Some(fragments) = &vmad.pack_fragments else {
            continue;
        };
        with_fragments += 1;
        saw_begin |= fragments.begin.is_some();
        saw_end |= fragments.end.is_some();
        saw_change |= fragments.change.is_some();
        assert_eq!(
            vmad.write(rec.tag),
            bytes,
            "PACK {id:?} VMAD did not reproduce the authored bytes"
        );
    }

    assert!(
        with_fragments > 0,
        "expected at least one installed PACK record with decoded fragments"
    );
    assert!(saw_begin, "expected an installed PACK begin fragment");
    assert!(saw_end, "expected an installed PACK end fragment");
    assert!(saw_change, "expected an installed PACK change fragment");
}
