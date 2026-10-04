// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-56 authoring, vanilla native operands and explicit expectation comparisons.

use std::{collections::BTreeMap, path::Path, process::Command};

use soe_papyrus::pex::{Instruction, Opcode, Operand};
use soe_probe::{build, script, FormKind, Observation, Probe, Step};

const SEQUENCE: &str = include_str!("../probes/anm_56.toml");
const EXPECTED: &str = include_str!("../probes/anm_56.expected.json");

fn probe() -> Probe {
    Probe::parse(SEQUENCE).unwrap()
}

fn calls(step: Step) -> Vec<Instruction> {
    let mut probe = probe();
    probe.steps = vec![step];
    script(&probe).unwrap().objects[0]
        .function_in("", "Step0")
        .unwrap()
        .code
        .iter()
        .filter(|i| matches!(i.op, Opcode::CallMethod | Opcode::CallStatic))
        .cloned()
        .collect()
}

#[test]
fn anm_56_event_batch_preserves_native_order_without_latent_waits() {
    let calls = calls(Step::AnimationEvents {
        id: "events".into(),
        actor: "player".into(),
        set_int: BTreeMap::default(),
        events: vec![
            "AttackWinStart".into(),
            "AttackWinEnd".into(),
            "blockStart".into(),
        ],
    });
    let events: Vec<_> = calls
        .iter()
        .filter(|call| call.op == Opcode::CallStatic)
        .map(|call| {
            assert_eq!(call.args[0], Operand::Identifier("Debug".into()));
            assert_eq!(
                call.args[1],
                Operand::Identifier("SendAnimationEvent".into())
            );
            assert_eq!(call.args[3], Operand::Identifier("f_player".into()));
            call.args[4].clone()
        })
        .collect();
    assert_eq!(
        events,
        [
            Operand::Str("AttackWinStart".into()),
            Operand::Str("AttackWinEnd".into()),
            Operand::Str("blockStart".into())
        ]
    );
    assert_eq!(calls.len(), 4, "three event calls and one observation");
}

#[test]
fn anm_56_animation_variable_calls_keep_integer_and_float_types() {
    for (step, setter, getter, value, destination) in [
        (
            Step::AnimationInt {
                id: "guard".into(),
                actor: "player".into(),
                variable: "iBlockState".into(),
                set: Some(1),
            },
            "SetAnimationVariableInt",
            "GetAnimationVariableInt",
            Operand::Int(1),
            "integer",
        ),
        (
            Step::AnimationFloat {
                id: "speed".into(),
                actor: "player".into(),
                variable: "weaponSpeedMult".into(),
                set: Some(0.0),
            },
            "SetAnimationVariableFloat",
            "GetAnimationVariableFloat",
            Operand::Float(0.0),
            "value",
        ),
    ] {
        let calls = calls(step);
        assert_eq!(calls[0].args[0], Operand::Identifier(setter.into()));
        assert_eq!(calls[0].args[4], value);
        assert_eq!(calls[1].args[0], Operand::Identifier(getter.into()));
        assert_eq!(calls[1].args[2], Operand::Identifier(destination.into()));
    }
    let calls = calls(Step::AnimationInt {
        id: "state".into(),
        actor: "player".into(),
        variable: "i1HMState".into(),
        set: None,
    });
    assert_eq!(
        calls[0].args[0],
        Operand::Identifier("GetAnimationVariableInt".into())
    );
    assert_eq!(calls.len(), 2, "read and observation only");
}

#[test]
fn anm_56_equipment_setup_adds_then_equips_a_vanilla_item() {
    let calls = calls(Step::EquipItem {
        id: "equip".into(),
        actor: "player".into(),
        item: "sword".into(),
    });
    assert_eq!(
        calls[0].args,
        [
            Operand::Identifier("AddItem".into()),
            Operand::Identifier("f_player".into()),
            Operand::Identifier("discard".into()),
            Operand::Identifier("f_sword".into()),
            Operand::Int(1),
            Operand::Bool(true)
        ]
    );
    assert_eq!(
        calls[1].args,
        [
            Operand::Identifier("EquipItem".into()),
            Operand::Identifier("f_player".into()),
            Operand::Identifier("discard".into()),
            Operand::Identifier("f_sword".into()),
            Operand::Bool(false),
            Operand::Bool(true)
        ]
    );
}

#[test]
fn anm_56_authoring_rejects_invalid_batches_variables_and_actor_types() {
    for events in [vec![], vec!["valid".into(); 17], vec!["bad event".into()]] {
        let mut probe = probe();
        probe.steps = vec![Step::AnimationEvents {
            id: "events".into(),
            actor: "player".into(),
            events,
            set_int: BTreeMap::default(),
        }];
        assert!(probe.validate().is_err());
    }
    for variable in ["", "bad\"variable"] {
        let mut probe = probe();
        probe.steps = vec![Step::AnimationInt {
            id: "int".into(),
            actor: "player".into(),
            variable: variable.into(),
            set: None,
        }];
        assert!(probe.validate().is_err());
    }
    let mut probe = probe();
    probe.steps = vec![Step::AnimationFloat {
        id: "float".into(),
        actor: "player".into(),
        variable: "Speed".into(),
        set: Some(f32::NAN),
    }];
    assert!(probe.validate().is_err());
    probe.steps = vec![Step::EquipItem {
        id: "equip".into(),
        actor: "player".into(),
        item: "sword".into(),
    }];
    probe.forms.get_mut("player").unwrap().kind = FormKind::Form;
    assert!(probe.validate().is_err());
}

#[test]
fn anm_56_batch_writes_guard_before_request_and_third_person_is_vanilla() {
    let calls = calls(Step::AnimationEvents {
        id: "request".into(),
        actor: "player".into(),
        events: vec!["InitiateWinEnd".into(), "blockStop".into()],
        set_int: std::collections::BTreeMap::from([("iBlockState".into(), 1)]),
    });
    assert_eq!(
        calls[0].args[0],
        Operand::Identifier("SetAnimationVariableInt".into())
    );
    assert_eq!(calls[0].args[3], Operand::Str("iBlockState".into()));
    assert_eq!(calls[0].args[4], Operand::Int(1));
    assert!(calls[1..3]
        .iter()
        .all(|call| call.args[1] == Operand::Identifier("SendAnimationEvent".into())));
    let camera = self::calls(Step::ThirdPerson {
        id: "camera".into(),
    });
    assert_eq!(
        camera[0].args,
        [
            Operand::Identifier("Game".into()),
            Operand::Identifier("ForceThirdPerson".into()),
            Operand::Identifier("discard".into())
        ]
    );
}

#[test]
fn anm_56_comparison_rejects_matching_wrong_authored_context() {
    let probe = probe();
    let mut expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    for observation in &mut expected {
        observation.forms.insert("runner".into(), 0x07001234);
    }
    let report = soe_probe::compare(&probe, &expected, &expected).unwrap();
    assert!(!report.matched);
    assert!(report
        .differences
        .iter()
        .any(|d| d.field == "engine.authored.forms.runner.object"));
}

#[test]
fn anm_56_sequence_builds_pex_plugin_and_seq() {
    let out = tempfile::tempdir().unwrap();
    build(&probe(), out.path()).unwrap();
    let bytes = std::fs::read(out.path().join("Scripts/SoeProbeRunner.pex")).unwrap();
    let script = soe_papyrus::pex::parse(&bytes).unwrap();
    assert_eq!(soe_papyrus::asm::write(&script), bytes);
    assert_eq!(probe().steps.len(), 60);
    assert_eq!(
        std::fs::read(out.path().join("Seq/SoeProbe.seq")).unwrap(),
        0x05000800u32.to_le_bytes()
    );
}

#[test]
fn anm_56_player_alias_uses_get_player_and_rejects_other_identities() {
    let mut probe = probe();
    let script = script(&probe).unwrap();
    let run = script.objects[0].function_in("", "RunSequence").unwrap();
    assert!(run.code.iter().any(|call| call.op == Opcode::CallStatic
        && call.args[..3]
            == [
                Operand::Identifier("Game".into()),
                Operand::Identifier("GetPlayer".into()),
                Operand::Identifier("f_player".into())
            ]));
    assert!(!run.code.iter().any(|call| call.op == Opcode::CallStatic
        && call.args[1] == Operand::Identifier("GetFormFromFile".into())
        && call.args[3] == Operand::Int(0x14)));
    probe.forms.get_mut("player").unwrap().object = 0x15;
    assert!(probe.validate().is_err());
    let player = probe.forms.get_mut("player").unwrap();
    player.object = 0x14;
    player.plugin = "ccBGSSSE001-Fish.esm".into();
    assert!(probe.validate().is_err());
}

#[test]
fn anm_56_cli_compares_expected_file_and_reports_state_difference() {
    let out = tempfile::tempdir().unwrap();
    let sequence = out.path().join("probe.toml");
    let expected = out.path().join("expected.json");
    let log = out.path().join("Papyrus.0.log");
    std::fs::write(&sequence, SEQUENCE).unwrap();
    std::fs::write(&expected, EXPECTED).unwrap();
    let mut observations: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    let write_log = |observations: &[Observation]| {
        let text = observations
            .iter()
            .fold(String::new(), |mut text, observation| {
                text.push_str(&format!(
                    "[authored synthetic fixture] SOEPROBE {}\n",
                    serde_json::to_string(observation).unwrap()
                ));
                text
            });
        std::fs::write(&log, text).unwrap();
    };
    write_log(&observations);
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_soe-probe"))
            .env_remove("SKYRIM_DATA")
            .args(["compare"])
            .arg(&sequence)
            .arg(&log)
            .arg(&expected)
            .output()
            .unwrap()
    };
    let matching = run();
    assert!(
        matching.status.success(),
        "{}",
        String::from_utf8_lossy(&matching.stderr)
    );
    let state = observations
        .iter_mut()
        .find(|o| o.request_id == "timeout_60_state")
        .unwrap();
    state.result.insert("value".into(), 99.0);
    write_log(&observations);
    let differing = run();
    assert_eq!(differing.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&differing.stdout).contains("result.value"));
    std::fs::write(&expected, "[").unwrap();
    assert_eq!(run().status.code(), Some(1));
}

#[test]
fn anm_56_installed_vanilla_forms_match_expectation_identities() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping ANM-56 form identities");
        return;
    };
    let probe = probe();
    let mut order = soe_esm::LoadOrder::new();
    for name in probe.load_order.iter().take(probe.load_order.len() - 1) {
        let plugin =
            soe_esm::Plugin::parse(&std::fs::read(Path::new(&data).join(name)).unwrap()).unwrap();
        order.add(name, plugin).unwrap();
    }
    order
        .add("SoeProbe.esp", soe_probe::plugin(&probe).unwrap())
        .unwrap();
    let game = soe_game::Game::new(order);
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    for (alias, form) in &probe.forms {
        let id = if form.kind == FormKind::Player {
            game.world_mut().player()
        } else {
            game.world()
                .form_from_file(form.object, &form.plugin)
                .unwrap()
        };
        assert_eq!(
            game.world().runtime_id(id) as i32,
            expected[0].forms[alias],
            "{alias}"
        );
    }
    assert_eq!(
        game.world().runtime_id(game.form("SoeProbe").unwrap()) as i32,
        expected[0].forms["runner"]
    );
}
