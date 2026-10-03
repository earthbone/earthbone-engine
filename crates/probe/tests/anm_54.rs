// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! ANM-54 transport checks use synthetic records and engine predictions.

use std::fmt::Write;

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_papyrus::pex::{Opcode, Operand};
use soe_probe::{compare, run, script, FormKind, Observation, Probe, Step};

const SEQUENCE: &str = include_str!("../probes/anm_54.toml");
const EXPECTED: &str = include_str!("../probes/anm_54.expected.json");

fn probe() -> Probe {
    Probe::parse(SEQUENCE).unwrap()
}

fn order(probe: &Probe) -> LoadOrder {
    let mut order = LoadOrder::new();
    for name in probe.load_order.iter().take(probe.load_order.len() - 1) {
        let mut plugin = Plugin::new(FileHeader {
            flags: flags::MASTER,
            ..FileHeader::default()
        });
        for form in probe
            .forms
            .values()
            .filter(|form| form.plugin == *name && form.kind != FormKind::Player)
        {
            plugin.push_top(Record::new(Tag(*b"MISC"), form.object));
        }
        if name == "Skyrim.esm" {
            plugin.push_top(
                Record::new(Tag(*b"GLOB"), 0x3a)
                    .zsub(Tag(*b"EDID"), "TimeScale")
                    .sub(Tag(*b"FNAM"), *b"f")
                    .sub(Tag(*b"FLTV"), 20.0_f32.to_le_bytes()),
            );
        }
        order.add(name, plugin).unwrap();
    }
    order
}

#[test]
fn anm_54_new_steps_execute_vanilla_natives_and_read_graph_variables() {
    let mut probe = probe();
    probe.animation_graph = None;
    probe.steps = vec![
        Step::ThirdPerson {
            id: "camera".into(),
        },
        Step::DrawWeapon {
            id: "draw".into(),
            actor: "player".into(),
        },
        Step::SetAnimationInt {
            id: "set".into(),
            actor: "player".into(),
            variable: "iBlockState".into(),
            value: 7,
        },
        Step::AnimationInt {
            id: "read".into(),
            actor: "player".into(),
            variable: "iBlockState".into(),
            set: None,
        },
        Step::AnimationBatch {
            id: "batch".into(),
            actor: "player".into(),
            events: vec!["blockStart".into(), "recoilStart".into()],
            ints: std::collections::BTreeMap::new(),
        },
    ];
    let observations = run(&probe, order(&probe)).unwrap();
    assert_eq!(observations[3].result["value"], 7.0);
    assert_eq!(observations[4].result["issued"], 1.0);
    assert_eq!(observations[3].forms["player"], 0x14);
}

#[test]
fn anm_54_batch_pex_preserves_authored_event_order_without_latent_calls() {
    let probe = probe();
    let script = script(&probe).unwrap();
    let object = &script.objects[0];
    let step = probe.steps.iter().position(|step| matches!(step, Step::AnimationBatch { id, .. } if id == "equal_priority_interrupt")).unwrap();
    let fun = object
        .states
        .iter()
        .flat_map(|state| &state.functions)
        .find(|fun| fun.name == format!("Step{step}"))
        .unwrap();
    let events: Vec<_> = fun
        .code
        .iter()
        .filter(|instruction| {
            instruction.op == Opcode::CallStatic
                && instruction.args.get(1)
                    == Some(&Operand::Identifier("SendAnimationEvent".into()))
        })
        .map(|instruction| instruction.args.last().unwrap().clone())
        .collect();
    assert_eq!(
        events,
        vec![
            Operand::Str("blockStart".into()),
            Operand::Str("recoilStart".into())
        ]
    );
}

#[test]
fn anm_54_delayed_batch_sets_admission_input_before_competing_events() {
    let probe = probe();
    let script = script(&probe).unwrap();
    let index = probe
        .steps
        .iter()
        .position(
            |step| matches!(step, Step::AnimationBatch { id, .. } if id == "delayed_last_attack"),
        )
        .unwrap();
    let function = script.objects[0].states[0]
        .functions
        .iter()
        .find(|function| function.name == format!("Step{index}"))
        .unwrap();
    let calls: Vec<_> = function
        .code
        .iter()
        .filter_map(|instruction| match instruction.op {
            Opcode::CallMethod => instruction.args[0].as_ident(),
            Opcode::CallStatic => instruction.args[1].as_ident(),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        [
            "SetAnimationVariableInt",
            "SendAnimationEvent",
            "SendAnimationEvent",
            "SendAnimationEvent",
            "Observe"
        ]
    );
}

#[test]
fn anm_54_new_steps_reject_invalid_actor_variable_batch_and_player_identity() {
    let invalid_steps = [
        Step::AnimationBatch {
            id: "bad".into(),
            actor: "runner".into(),
            events: vec!["blockStart".into()],
            ints: std::collections::BTreeMap::new(),
        },
        Step::AnimationBatch {
            id: "bad".into(),
            actor: "player".into(),
            events: vec![],
            ints: std::collections::BTreeMap::new(),
        },
        Step::AnimationBatch {
            id: "bad".into(),
            actor: "player".into(),
            events: vec!["blockStart".into(); 17],
            ints: std::collections::BTreeMap::new(),
        },
        Step::AnimationInt {
            id: "bad".into(),
            actor: "player".into(),
            variable: "bad variable".into(),
            set: None,
        },
        Step::SetAnimationInt {
            id: "bad".into(),
            actor: "runner".into(),
            variable: "iBlockState".into(),
            value: 1,
        },
        Step::DrawWeapon {
            id: "bad".into(),
            actor: "runner".into(),
        },
    ];
    for step in invalid_steps {
        let mut probe = probe();
        probe.steps = vec![step];
        assert!(probe.validate().is_err());
    }
    let mut probe = probe();
    probe.forms.get_mut("player").unwrap().object = 7;
    assert!(probe
        .validate()
        .unwrap_err()
        .to_string()
        .contains("player must name"));
}

#[test]
fn anm_54_reviewed_predictions_reject_changed_state_missing_and_reordered_boundaries() {
    let probe = probe();
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    assert!(compare(&probe, &expected, &expected).unwrap().matched);
    let mut changed = expected.clone();
    changed
        .iter_mut()
        .find(|sample| sample.request_id == "local_wildcard_result")
        .unwrap()
        .result
        .insert("value".into(), 12.0);
    assert!(!compare(&probe, &expected, &changed).unwrap().matched);
    let mut missing = expected.clone();
    missing.pop();
    assert!(!compare(&probe, &expected, &missing).unwrap().matched);
    let mut reordered = expected.clone();
    reordered.swap(0, 1);
    assert!(!compare(&probe, &expected, &reordered).unwrap().matched);
}

#[test]
fn anm_54_cli_compares_expected_file_without_installed_data() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("Papyrus.0.log");
    let expected: Vec<Observation> = serde_json::from_str(EXPECTED).unwrap();
    let mut text = String::new();
    for sample in &expected {
        writeln!(text, "SOEPROBE {}", serde_json::to_string(sample).unwrap()).unwrap();
    }
    std::fs::write(&log, text).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_soe-probe"))
        .env_remove("SKYRIM_DATA")
        .args(["compare", "probes/anm_54.toml"])
        .arg(&log)
        .arg("probes/anm_54.expected.json")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
