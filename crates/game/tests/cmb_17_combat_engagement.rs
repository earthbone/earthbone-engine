// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-17 combat target acquisition, navmesh pursuit and disengagement.

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionLevel, Motion, Observation};
use soe_game::package::{self, Schedule};
use soe_game::World;

const CELL: u32 = 0x800;
const HUNTER_BASE: u32 = 0x801;
const TARGET_BASE: u32 = 0x802;
const IGNORE_COMBAT_PACKAGE: u32 = 0x850;
const HUNTER: u32 = 0x900;
const TARGET: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn actor_base(object: u32, aggression: u8) -> Record {
    let mut ai_data = vec![aggression, 4, 70, 1, 3, 2, 0, 0];
    for radius in [500_u32, 300, 100] {
        ai_data.extend_from_slice(&radius.to_le_bytes());
    }
    Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"AIDT"), ai_data)
}

fn actor_ref(object: u32, base: u32, position: [f32; 3]) -> Record {
    let mut data = Vec::with_capacity(24);
    for value in position {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.extend_from_slice(&[0_u8; 12]);
    Record::new(Tag(*b"ACHR"), object)
        .sub(Tag(*b"NAME"), base.to_le_bytes())
        .sub(Tag(*b"DATA"), data)
}

fn navmesh() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(CELL),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [100.0, 0.0, 0.0],
            [100.0, 100.0, 0.0],
            [0.0, 100.0, 0.0],
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
            size: [100.0, 100.0],
            min: [0.0, 0.0, 0.0],
            max: [100.0, 100.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

fn gmst_f(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

fn gmst_i(object: u32, name: &str, value: i32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

fn fixture() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(actor_base(HUNTER_BASE, 1));
    plugin.push_top(actor_base(TARGET_BASE, 0));
    plugin.push_top(actor_ref(HUNTER, HUNTER_BASE, [5.0, 10.0, 0.0]));
    plugin.push_top(actor_ref(TARGET, TARGET_BASE, [80.0, 80.0, 0.0]));
    plugin.push_top(Record::new(Tag(*b"NAVM"), 0x810).sub(Tag(*b"NVNM"), navmesh().to_bytes()));
    plugin.push_top(package::with_actions(package::finish(
        package::build_record(
            IGNORE_COMBAT_PACKAGE,
            "IgnoreCombat",
            package::flags::IGNORE_COMBAT,
            Schedule::default(),
        ),
    )));
    let float_settings = [
        ("fSneakBaseValue", 0.0),
        ("fSneakMaxDistance", 100.0),
        ("fSneakDistanceAttenuationExponent", 1.0),
        ("fSneakSoundsMult", 1.0),
        ("fSneakSoundLosMult", 1.0),
        ("fSneakEquippedWeightBase", 0.0),
        ("fSneakEquippedWeightMult", 1.0),
        ("fSneakRunningMult", 1.0),
        ("fSneakActionMult", 1.0),
        ("fSneakLightMoveMult", 0.0),
        ("fSneakLightRunMult", 0.0),
        ("fDetectionSneakLightMod", 0.0),
        ("fSneakLightMult", 1.0),
        ("fSneakStealthBoyMult", 1.0),
        ("fSneakPerceptionSkillMin", 0.0),
        ("fSneakPerceptionSkillMax", 100.0),
        ("fSneakAlertMod", 0.0),
        ("fSneakCombatMod", 0.0),
        ("fCombatStealthPointMax", 100.0),
        ("fCombatStealthPointDrainMult", 1.0),
        ("fCombatStealthPointRegenMult", 1.0),
        ("fCombatStealthPointRegenMin", 1.0),
        ("fCombatStealthPointRegenAlertWaitTime", 5.0),
        ("fCombatStealthPointRegenLostWaitTime", 5.0),
    ];
    for (index, (name, value)) in float_settings.into_iter().enumerate() {
        plugin.push_top(gmst_f(0x1000 + index as u32, name, value));
    }
    plugin.push_top(gmst_i(0x1100, "iCombatStealthPointDetectionThreshold", 50));
    plugin.push_top(gmst_i(
        0x1101,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));
    let mut order = LoadOrder::new();
    order.add("CombatEngagement.esm", plugin).unwrap();
    World::new(order)
}

fn detect_target(world: &mut World) {
    world
        .evaluate_detection(
            &[Observation {
                observer: id(HUNTER),
                target: id(TARGET),
                distance: 1.0,
                line_of_sight: true,
                light_level: 100.0,
                motion: Motion::Still,
                action_sound: 0.0,
                action_sound_distance: None,
                action_sound_line_of_sight: None,
            }],
            1.0,
        )
        .unwrap();
    assert_eq!(
        world.awareness(id(HUNTER), id(TARGET)).unwrap().level,
        DetectionLevel::Combat
    );
}

#[test]
fn cmb_17_hostile_actor_acquires_its_detected_target() {
    let mut world = fixture();
    detect_target(&mut world);
    world.ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));

    let hunter = world.actor(id(HUNTER)).unwrap();
    assert!(hunter.in_combat);
    assert_eq!(hunter.combat_target, Some(id(TARGET)));
}

#[test]
fn cmb_17_actor_closes_on_target_over_navmesh() {
    let mut world = fixture();
    detect_target(&mut world);
    for _ in 0..5 {
        world.ai_tick(0.1, &mut |_, _| panic!("unexpected package script binding"));
    }

    assert!(
        world.ai.nav_graph().is_some(),
        "the NAVM graph was not built"
    );
    let position = world.reference(id(HUNTER)).unwrap().position;
    let target = world.reference(id(TARGET)).unwrap().position;
    let remaining = ((position[0] - target[0]).powi(2)
        + (position[1] - target[1]).powi(2)
        + (position[2] - target[2]).powi(2))
    .sqrt();
    assert!(
        remaining < 10.0,
        "combatant did not close on target: {remaining}"
    );
}

#[test]
fn cmb_17_actor_leaves_combat_when_target_dies() {
    let mut world = fixture();
    detect_target(&mut world);
    world.ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));
    assert!(world.kill_actor(id(TARGET), Some(id(HUNTER))));
    world.ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));

    let hunter = world.actor(id(HUNTER)).unwrap();
    assert!(!hunter.in_combat);
    assert_eq!(hunter.combat_target, None);
}

#[test]
fn cmb_17_ignore_combat_package_keeps_package_driver_in_control() {
    let mut world = fixture();
    detect_target(&mut world);
    world.actor(id(HUNTER)).unwrap().ai.package = Some(id(IGNORE_COMBAT_PACKAGE));
    world.ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));

    let hunter = world.actor(id(HUNTER)).unwrap();
    assert!(!hunter.in_combat);
    assert_eq!(hunter.combat_target, None);
}

#[test]
fn cmb_17_unaggressive_actor_does_not_acquire_a_target() {
    let mut world = fixture();
    world
        .set_actor_value(id(HUNTER), "aggression", 0.0)
        .unwrap();
    detect_target(&mut world);
    world.ai_tick(0.0, &mut |_, _| panic!("unexpected package script binding"));

    let hunter = world.actor(id(HUNTER)).unwrap();
    assert!(!hunter.in_combat);
    assert_eq!(hunter.combat_target, None);
}
