// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-09 Actor queries backed by race and shared detection state.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::detection::{DetectionLevel, Error as DetectionError};
pub(super) use soe_game::detection::{Motion, Observation};
use soe_game::Game;
use soe_papyrus::asm::{b, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::{Opcode::CallMethod, Value};

pub(super) fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn race(object: u32, child: bool) -> Record {
    let mut data = vec![0; 124];
    let flags = if child { 0x4_u32 } else { 0 };
    data[32..36].copy_from_slice(&flags.to_le_bytes());
    Record::new(Tag(*b"RACE"), object).sub(Tag(*b"DATA"), data)
}

fn detection_records() -> Vec<Record> {
    let values: [(&str, f32); 24] = [
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
    let mut records: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, (name, value))| {
            Record::new(Tag(*b"GMST"), 0x1000 + i as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), (*value).to_le_bytes())
        })
        .collect();
    for (i, (name, value)) in [
        ("iCombatStealthPointDetectionThreshold", 50_i32),
        ("iCombatStealthPointSneakDetectionThreshold", 70_i32),
    ]
    .into_iter()
    .enumerate()
    {
        records.push(
            Record::new(Tag(*b"GMST"), 0x1100 + i as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    records
}

pub(super) fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x800, true));
    plugin.push_top(race(0x801, false));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x810).sub(Tag(*b"RNAM"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x811).sub(Tag(*b"RNAM"), 0x801_u32.to_le_bytes()));
    for (actor, base) in [(0x900_u32, 0x810_u32), (0x901, 0x811), (0x902, 0x811)] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), base.to_le_bytes()));
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x910));
    for record in detection_records() {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("SCR09.esm", plugin).unwrap();
    Game::new(order)
}

pub(super) fn actor_bool(
    game: &mut Game,
    actor: GlobalFormId,
    method: &str,
    args: &[Value],
) -> bool {
    let class = format!("Scr09{method}");
    let function = format!("Run{method}");
    let mut code = vec![s(method), ident("self"), ident("result")];
    let mut builder = FunctionBuilder::new(&function, "None");
    for (index, _) in args.iter().enumerate() {
        let name = format!("arg{index}");
        builder = builder.param(&name, "Var");
        code.push(ident(&name));
    }
    game.load_script(
        &ScriptBuilder::new(&class, "Actor")
            .variable("result", "Bool", b(false))
            .function(builder.ins(CallMethod, code).build())
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(actor);
    game.vm.attach(object, &class).unwrap();
    game.vm.call(object, &function, args).unwrap();
    assert!(game.run_until_idle(32));
    game.vm.get_var(object, &class, "result").unwrap().as_bool()
}

#[test]
fn scr_09_is_child_uses_actor_race_data_through_native_and_world_api() {
    let mut game = fixture();
    assert!(game.world_mut().actor_is_child(id(0x900)).unwrap());
    assert!(!game.world_mut().actor_is_child(id(0x901)).unwrap());
    assert!(actor_bool(&mut game, id(0x900), "IsChild", &[]));
    assert!(!actor_bool(&mut game, id(0x901), "IsChild", &[]));
    assert!(game.world_mut().actor_is_child(id(0x910)).is_err());
}

#[test]
fn scr_09_is_detected_by_uses_other_actor_and_does_not_advance_state() {
    let mut game = fixture();
    let observation = Observation {
        observer: id(0x901),
        target: id(0x900),
        distance: 0.0,
        line_of_sight: true,
        light_level: 100.0,
        motion: Motion::Still,
        action_sound: 0.0,
        action_sound_distance: None,
        action_sound_line_of_sight: None,
    };
    game.world_mut()
        .evaluate_detection(&[observation], 1.0)
        .unwrap();
    let before = game.world().awareness(id(0x901), id(0x900)).unwrap();
    assert_eq!(before.level, DetectionLevel::Combat);
    assert!(game
        .world()
        .is_actor_detected_by(id(0x900), id(0x901))
        .unwrap());
    let observer = game.object_of_form(id(0x901));
    assert!(actor_bool(
        &mut game,
        id(0x900),
        "IsDetectedBy",
        &[Value::Object(observer)],
    ));
    assert!(!actor_bool(
        &mut game,
        id(0x900),
        "IsDetectedBy",
        &[Value::None],
    ));
    assert_eq!(game.world().awareness(id(0x901), id(0x900)), Some(before));
    assert!(matches!(
        game.world()
            .is_actor_detected_by(id(0x900), id(0x910)),
        Err(DetectionError::Actor(actor)) if actor == id(0x910)
    ));
}
