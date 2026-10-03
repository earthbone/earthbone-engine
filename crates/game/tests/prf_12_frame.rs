// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-12 frame accounting and calibration. The 1000-actor records match
//! crates/bench/src/stress/fixtures.rs through the same public form builders.

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{npc, package, Game};
use soe_trace::{with_timing_suspended, FrameSnapshot, System};
use std::time::Instant;

const CELL: u32 = 0x800;
const BASE: u32 = 0x801;
const TEMPLATE: u32 = 0x802;
const PACKAGE: u32 = 0x803;
const MARKER: u32 = 0x804;
const NAV: u32 = 0x805;
const SANDBOX_BASE: u32 = 0x806;
const SANDBOX_TEMPLATE: u32 = 0x807;
const SANDBOX_PACKAGE: u32 = 0x808;
const ACTOR: u32 = 0x1000;
const FAR: f32 = 100_000_000.0;
const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn placed(kind: [u8; 4], id: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data: Vec<u8> = position.into_iter().flat_map(f32::to_le_bytes).collect();
    data.extend_from_slice(&[0; 12]);
    Record::new(T(&kind), id)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), data)
}

fn actors(count: usize) -> Result<(Game, Vec<GlobalFormId>), String> {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::build_template(TEMPLATE, "StressTravelTemplate");
    let template = package::close_header(template, None);
    let template = package::with_location_input(template, 0, package::Location::NearSelf, 0);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Travel", &[0], true);
    let template = package::with_input_name(template, 0, "Place to Travel", true);
    plugin.push_top(package::with_actions(template));
    let sandbox_template = package::build_template(SANDBOX_TEMPLATE, "StressSandboxTemplate");
    let sandbox_template = package::close_header(sandbox_template, None);
    let sandbox_template =
        package::with_location_input(sandbox_template, 0, package::Location::NearSelf, 200);
    let sandbox_template = package::finish(sandbox_template);
    let sandbox_template = package::with_procedure(sandbox_template, "Sandbox", &[0], true);
    let sandbox_template = package::with_input_name(sandbox_template, 0, "Location", true);
    plugin.push_top(package::with_actions(sandbox_template));
    let travel = package::build_record(PACKAGE, "StressTravel", 0, package::Schedule::default());
    let travel = package::close_header(travel, Some(TEMPLATE));
    let travel =
        package::with_location_input(travel, 0, package::Location::NearReference(MARKER), 0);
    plugin.push_top(package::with_actions(package::finish(travel)));
    plugin.push_top(
        npc::build_record(
            BASE,
            "StressActor",
            "Stress actor",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        npc::build_record(
            SANDBOX_BASE,
            "StressSandboxActor",
            "Stress sandbox actor",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(T(b"PKID"), SANDBOX_PACKAGE.to_le_bytes()),
    );
    let sandbox = package::build_record(
        SANDBOX_PACKAGE,
        "StressSandbox",
        0,
        package::Schedule::default(),
    );
    let sandbox = package::close_header(sandbox, Some(SANDBOX_TEMPLATE));
    let sandbox = package::with_location_input(sandbox, 0, package::Location::NearSelf, 200);
    plugin.push_top(package::with_actions(package::finish(sandbox)));
    let cell = plugin.push_record(Record::new(T(b"CELL"), CELL).sub(T(b"DATA"), [1, 0]));
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    let geometry = Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(CELL),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [FAR, 0.0, 0.0],
            [FAR, 1000.0, 0.0],
            [0.0, 1000.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, 1],
                flags: 0,
                cover_flags: 0,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [FAR, 1000.0],
            min: [0.0; 3],
            max: [FAR, 1000.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    };
    let records = [
        placed(*b"REFR", MARKER, BASE, [FAR - 100.0, 500.0, 0.0]),
        Record::new(T(b"NAVM"), NAV).sub(T(b"NVNM"), geometry.to_bytes()),
    ];
    for record in records {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(record)));
    }
    for n in 0..count {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(placed(
                *b"ACHR",
                ACTOR + n as u32,
                if n % 2 == 0 { BASE } else { SANDBOX_BASE },
                [10.0, 100.0 + (n % 800) as f32, 0.0],
            ))));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block
        .entries
        .extend([Entry::Record(cell), Entry::Group(children)]);
    plugin.groups.push(block);
    let mut order = LoadOrder::new();
    order.add("Stress.esm", plugin).map_err(|e| e.to_string())?;
    let game = Game::new(order);
    let ids: Vec<_> = (0..count)
        .map(|n| GlobalFormId {
            file: 0,
            object: ACTOR + n as u32,
        })
        .collect();
    for &id in &ids {
        if !game.world_mut().drive_actor(id) {
            return Err(format!("could not drive actor {id}"));
        }
    }
    Ok((game, ids))
}

#[test]
fn prf_12_tick_reports_every_system_and_resets_idle_frames() {
    let (mut game, ids) = actors(2).unwrap();
    game.tick(1.0 / 60.0);
    let sample = game.frame_history().last();
    for system in [
        System::Papyrus,
        System::AiPlanning,
        System::AiMovement,
        System::Magic,
        System::Quests,
        System::Dialogue,
        System::Other,
    ] {
        assert!(sample.system_ns[system as usize] > 0, "{}", system.name());
    }
    assert!(sample.system_ns.iter().sum::<u64>() <= sample.frame_ns);
    assert!(game.world().reference_ref(ids[0]).unwrap().position[0] > 10.0);
    for system in System::ALL {
        assert!(game.frame_history().summary_text().contains(system.name()));
    }
    game.tick_menu_mode(1.0 / 60.0, true);
    assert_eq!(
        game.frame_history().last().system_ns[System::AiMovement as usize],
        0
    );
    assert_eq!(game.frame_history().frames(), 2);
    game.world_mut()
        .set_actor_value(ids[0], "aggression", 2.0)
        .unwrap();
    game.tick(1.0 / 60.0);
    assert!(game.frame_history().last().system_ns[System::Combat as usize] > 0);
}

#[test]
fn prf_12_nested_game_frames_preserve_host_save_accounting() {
    let mut game = Game::new(LoadOrder::new());
    let host = FrameSnapshot::now();
    game.tick(1.0 / 60.0);
    let save = game.save_state().unwrap();
    assert!(!save.is_empty());
    game.world_mut().play_sound(
        GlobalFormId {
            file: 0,
            object: 0x900,
        },
        GlobalFormId {
            file: 0,
            object: 0x14,
        },
    );
    let host_sample = host.finish();
    assert!(host_sample.system_ns[System::Save as usize] > 0);
    assert!(host_sample.system_ns[System::Audio as usize] > 0);
    assert_eq!(
        game.frame_history().last().system_ns[System::Save as usize],
        0
    );
    assert!(
        host_sample.system_ns[System::Papyrus as usize]
            >= game.frame_history().last().system_ns[System::Papyrus as usize]
    );
}

#[test]
#[ignore = "requires an optimized build and an idle machine"]
fn prf_12_accumulator_under_one_percent_of_thousand_actor_stress_tick() {
    const MAX_RATIO: f64 = 1.01;
    const PAIRS: usize = 15;
    const TICKS: usize = 8;
    let (mut measured, ids) = actors(1000).unwrap();
    let (mut baseline, _) = actors(1000).unwrap();
    for _ in 0..10 {
        measured.tick(1.0 / 60.0);
    }
    let mut ratios = Vec::with_capacity(PAIRS);
    for pair in 0..PAIRS {
        let mut times = [0.0; 2];
        // CONTEXT: Alternate order to distribute warm-cache and scheduling bias.
        for slot in if pair % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            if slot == 0 {
                for _ in 0..TICKS {
                    measured.tick(1.0 / 60.0);
                }
            } else {
                with_timing_suspended(|| {
                    for _ in 0..TICKS {
                        measured.tick(1.0 / 60.0);
                    }
                });
            }
            times[slot] = start.elapsed().as_secs_f64();
        }
        ratios.push(times[0] / times[1]);
    }
    ratios.sort_by(f64::total_cmp);
    let ratio = ratios[PAIRS / 2];
    eprintln!(
        "PRF-12 1000 actors: median on/off {ratio:.6}, overhead {:.3}%",
        (ratio - 1.0) * 100.0
    );
    assert!(
        ratio < MAX_RATIO,
        "accumulator ratio {ratio:.6} exceeds {MAX_RATIO}"
    );
    with_timing_suspended(|| {
        for _ in 0..10 + PAIRS * TICKS * 2 {
            baseline.tick(1.0 / 60.0);
        }
    });
    for id in ids {
        assert_eq!(
            measured.world().reference_ref(id).unwrap().position,
            baseline.world().reference_ref(id).unwrap().position
        );
    }
}
