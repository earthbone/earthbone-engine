// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-30 Guard package area holding, conditions and intrusion response.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::detection::{DetectionLevel, Motion, Observation};
use soe_game::package::{self, condition, CompValue, Condition, Location, Schedule};
use soe_game::World;
use soe_papyrus::vm::state::{Reader, Writer};

const CELL: u32 = 0x800;
const GUARD_BASE: u32 = 0x801;
const INTRUDER_BASE: u32 = 0x802;
const POST: u32 = 0x803;
const GUARD_PACKAGE_TEMPLATE: u32 = 0x850;
const GUARD_PACKAGE: u32 = 0x851;
const GUARD: u32 = 0x900;
const INTRUDER: u32 = 0x901;

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

fn guard_world() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), CELL));
    plugin.push_top(actor_base(GUARD_BASE, 1).sub(Tag(*b"PKID"), GUARD_PACKAGE.to_le_bytes()));
    plugin.push_top(actor_base(INTRUDER_BASE, 0));
    plugin.push_top(actor_ref(GUARD, GUARD_BASE, [5.0, 10.0, 0.0]));
    plugin.push_top(actor_ref(INTRUDER, INTRUDER_BASE, [80.0, 80.0, 0.0]));
    plugin.push_top(actor_ref(POST, INTRUDER_BASE, [20.0, 20.0, 0.0]));

    let template = package::build_template(GUARD_PACKAGE_TEMPLATE, "GuardTemplate");
    let template = package::close_header(template, None);
    let template = package::with_location_input(template, 0, Location::NearReference(POST), 10);
    let template = package::finish(template);
    let template = package::with_procedure(template, "Guard", &[0], true);
    let template = package::with_input_name(template, 0, "Guard Area", true);
    plugin.push_top(package::with_actions(package::finish(template)));

    let condition = Condition {
        comparison: condition::GREATER_OR_EQUAL,
        flags: 0,
        value: CompValue::Float(1.0),
        function: 80,
        param1: 0,
        param2: 0,
        run_on: 0,
        reference: 0,
        param3: -1,
    };
    let package_record =
        package::build_record(GUARD_PACKAGE, "GuardPackage", 0, Schedule::default());
    let package_record = package::with_condition(package_record, &condition);
    let package_record = package::close_header(package_record, Some(GUARD_PACKAGE_TEMPLATE));
    let package_record =
        package::with_location_input(package_record, 0, Location::NearReference(POST), 10);
    plugin.push_top(package::with_actions(package::finish(package_record)));

    for (index, (name, value)) in [
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
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(gmst_f(0x1000 + index as u32, name, value));
    }
    plugin.push_top(gmst_i(0x1100, "iCombatStealthPointDetectionThreshold", 50));
    plugin.push_top(gmst_i(
        0x1101,
        "iCombatStealthPointSneakDetectionThreshold",
        70,
    ));

    let mut order = LoadOrder::new();
    order.add("Ai30Guard.esm", plugin).unwrap();
    let mut world = World::new(order);
    world.actor(id(GUARD)).unwrap().level = 5;
    world.ai_tick(0.0, &mut |_, _| soe_papyrus::value::ObjectId(0));
    assert_eq!(
        world.actor(id(GUARD)).unwrap().ai.procedure,
        Procedure::Guard,
        "the authored package condition selected the Guard procedure"
    );
    assert_eq!(
        world.actor(id(GUARD)).unwrap().ai.center,
        Some([20.0, 20.0, 0.0])
    );
    world
}

fn detect_intruder(world: &mut World) {
    world
        .evaluate_detection(
            &[Observation {
                observer: id(GUARD),
                target: id(INTRUDER),
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
        world.awareness(id(GUARD), id(INTRUDER)).unwrap().level,
        DetectionLevel::Combat
    );
}

#[test]
fn ai_30_guard_holds_its_assigned_area_and_reacts_only_to_eligible_intrusions() {
    let mut world = guard_world();
    let start = world.reference(id(GUARD)).unwrap().position;
    world.ai_tick(0.2, &mut |_, _| soe_papyrus::value::ObjectId(0));
    let moved = world.reference(id(GUARD)).unwrap().position;
    assert!(moved[0] > start[0] && moved[1] > start[1]);
    for _ in 0..30 {
        world.ai_tick(0.1, &mut |_, _| soe_papyrus::value::ObjectId(0));
    }
    let at_post = world.reference(id(GUARD)).unwrap().position;
    assert!(
        (at_post[0] - 20.0).abs() < 5.0 && (at_post[1] - 20.0).abs() < 5.0,
        "the actor did not reach the authored post: {at_post:?}"
    );

    detect_intruder(&mut world);
    world.ai_tick(0.0, &mut |_, _| soe_papyrus::value::ObjectId(0));
    assert!(
        !world.actor(id(GUARD)).unwrap().in_combat,
        "a detected actor outside the guard area is not an eligible intrusion"
    );

    world.reference(id(INTRUDER)).unwrap().position = [25.0, 20.0, 0.0];
    detect_intruder(&mut world);
    world.ai_tick(0.0, &mut |_, _| soe_papyrus::value::ObjectId(0));
    let actor = world.actor(id(GUARD)).unwrap();
    assert!(actor.in_combat);
    assert_eq!(actor.combat_target, Some(id(INTRUDER)));
}

#[test]
fn ai_30_authored_package_conditions_can_disable_guarding() {
    let mut world = guard_world();
    let guard = world.actor(id(GUARD)).unwrap();
    guard.level = 0;
    guard.ai.reevaluate = true;
    world.ai_tick(0.0, &mut |_, _| soe_papyrus::value::ObjectId(0));

    let guard = world.actor(id(GUARD)).unwrap();
    assert_eq!(guard.ai.package, None);
    assert_eq!(guard.ai.procedure, Procedure::None);
}

#[test]
fn ai_30_guard_driver_state_survives_save_roundtrip() {
    let mut world = guard_world();
    let state = world.actor(id(GUARD)).unwrap().ai.clone();
    let mut writer = Writer::new();
    soe_game::ai::write_state(&mut writer, &state);

    let restored = soe_game::ai::read_state(&mut Reader::new(&writer.out)).unwrap();
    assert_eq!(restored, state);
}
