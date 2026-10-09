// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! TOOL-04 checks use authored records and a synthetic Papyrus log.
//! Installed masters remain external inputs behind SKYRIM_DATA.

use std::{collections::BTreeMap, process::Command};

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_papyrus::pex::{self, ByteOrder};
use soe_probe::{
    build, compare, parse_log, plugin, run, run_installed, script, Error, Form, FormKind, Menu,
    Probe, Step, EXAMPLE,
};

const LOG: &str = include_str!("fixtures/Papyrus.0.txt");

fn probe() -> Probe {
    Probe::parse(EXAMPLE).unwrap()
}

fn synthetic_order(probe: &Probe) -> LoadOrder {
    let mut order = LoadOrder::new();
    for name in probe.load_order.iter().take(probe.load_order.len() - 1) {
        let mut plugin = Plugin::new(FileHeader {
            flags: flags::MASTER
                | if std::path::Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("esl"))
                {
                    flags::LIGHT_MASTER
                } else {
                    0
                },
            ..FileHeader::default()
        });
        for (alias, form) in &probe.forms {
            if alias.starts_with("anchor_") && form.plugin == *name {
                plugin.push_top(Record::new(Tag(*b"MISC"), form.object));
            }
        }
        if name == "Skyrim.esm" {
            plugin.push_top(
                Record::new(Tag(*b"GLOB"), 0x3a)
                    .zsub(Tag(*b"EDID"), "TimeScale")
                    .sub(Tag(*b"FNAM"), *b"f")
                    .sub(Tag(*b"FLTV"), probe.timescale.to_le_bytes()),
            );
            plugin.push_top(soe_game::npc::build_record(
                0x500,
                "ProbeActor",
                "Probe Actor",
                soe_game::npc::Config::default(),
                soe_game::npc::Stats {
                    health: 100,
                    ..soe_game::npc::Stats::default()
                },
            ));
            plugin.push_top(
                Record::new(Tag(*b"ACHR"), 0x100)
                    .sub(Tag(*b"NAME"), 0x500u32.to_le_bytes())
                    .sub(Tag(*b"DATA"), vec![0; 24]),
            );
            let mut effect = vec![0; 152];
            effect[..4].copy_from_slice(&2u32.to_le_bytes());
            effect[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
            effect[68..72].copy_from_slice(&24u32.to_le_bytes());
            effect[80..84].copy_from_slice(&1u32.to_le_bytes());
            effect[84..88].copy_from_slice(&3u32.to_le_bytes());
            plugin.push_top(Record::new(Tag(*b"MGEF"), 0x200).sub(Tag(*b"DATA"), effect));
            let mut spit = vec![0; 36];
            spit[16..20].copy_from_slice(&1u32.to_le_bytes());
            spit[20..24].copy_from_slice(&3u32.to_le_bytes());
            let mut efit = 10f32.to_le_bytes().to_vec();
            efit.extend(0u32.to_le_bytes());
            efit.extend(5u32.to_le_bytes());
            plugin.push_top(
                Record::new(Tag(*b"SPEL"), 0x300)
                    .sub(Tag(*b"SPIT"), spit)
                    .sub(Tag(*b"EFID"), 0x200u32.to_le_bytes())
                    .sub(Tag(*b"EFIT"), efit),
            );
        }
        order.add(name, plugin).unwrap();
    }
    order
}

#[test]
fn tool_04_authored_sequence_runs_and_compares_synthetic_trace() {
    let probe = probe();
    let engine = run(&probe, synthetic_order(&probe)).unwrap();
    let observed = parse_log(LOG).unwrap();
    let report = compare(&probe, &engine, &observed).unwrap();
    assert!(report.matched, "{:?}", report.differences);
    assert_eq!(engine.last().unwrap().result["stage"], 20.0);
}

#[test]
fn tool_04_build_emits_vanilla_quest_vmad_pex_and_seq() {
    let probe = probe();
    let out = tempfile::tempdir().unwrap();
    build(&probe, out.path()).unwrap();
    let plugin = Plugin::parse(&std::fs::read(out.path().join("SoeProbe.esp")).unwrap()).unwrap();
    assert_eq!(plugin.header.masters, probe.load_order[..5]);
    assert_eq!(plugin.header.record_count, 2);
    let quest = &plugin.records[0];
    let definition = soe_game::quest::Quest::from_record(quest, false).unwrap();
    assert!(definition.starts_enabled());
    assert!(definition.stage(10).is_some());
    assert!(definition.stage(20).is_some());
    let vmad =
        soe_game::vmad::Vmad::parse(quest.find(Tag(*b"VMAD")).unwrap(), Tag(*b"QUST")).unwrap();
    assert_eq!(vmad.scripts[0].name, "SoeProbeRunner");
    assert_eq!(
        std::fs::read(out.path().join("Seq/SoeProbe.seq")).unwrap(),
        0x05000800u32.to_le_bytes()
    );
    let bytes = std::fs::read(out.path().join("Scripts/SoeProbeRunner.pex")).unwrap();
    assert_eq!(&bytes[..8], &[0xfa, 0x57, 0xc0, 0xde, 3, 2, 0, 1]);
    let pex = pex::parse(&bytes).unwrap();
    assert_eq!(pex.header.byte_order, ByteOrder::Big);
    assert_eq!(pex.header.source, "SoeProbeRunner.psc");
    assert_eq!(
        pex.user_flags
            .iter()
            .map(|flag| (flag.name.as_str(), flag.bit))
            .collect::<Vec<_>>(),
        [("hidden", 0), ("conditional", 1)]
    );
    assert_eq!(pex.objects[0].parent, "Quest");
    assert!(pex.objects[0].states[0]
        .functions
        .iter()
        .all(|fun| fun.lines.len() == fun.code.len()));
    assert_eq!(soe_papyrus::asm::write(&pex), bytes);
}

#[test]
fn tool_04_parse_preserves_request_identity_context_and_noise() {
    let observations = parse_log(LOG).unwrap();
    assert_eq!(observations.len(), 7);
    assert_eq!(observations[1].phase, "begin");
    assert_eq!(observations[2].request_id, "wait_1");
    assert_eq!(observations[2].runtime_version, "1.6.1170.0");
    assert_eq!(observations[2].forms["runner"], 0x07000800);
    assert_eq!(observations[2].load_order, probe().load_order);
    assert_eq!(observations[2].skse_version, None);
    assert_eq!(observations[2].paused, 0);
}

#[test]
fn tool_04_parse_rejects_malformed_duplicate_and_empty_logs() {
    for input in [
        "unrelated",
        "[timestamp] SOEPROBE {}",
        "SOEPROBE {truncated",
    ] {
        assert!(
            matches!(parse_log(input), Err(Error::Observation { .. })),
            "{input}"
        );
    }
    assert!(parse_log(&format!("{LOG}{LOG}"))
        .unwrap_err()
        .to_string()
        .contains("duplicate"));
    let mut value = serde_json::to_value(&parse_log(LOG).unwrap()[0]).unwrap();
    value["unknown"] = true.into();
    assert!(parse_log(&format!("SOEPROBE {value}")).is_err());
    value.as_object_mut().unwrap().remove("unknown");
    value["paused"] = 2.into();
    assert!(parse_log(&format!("SOEPROBE {value}")).is_err());
}

#[test]
fn tool_04_compare_applies_each_declared_numeric_tolerance() {
    let mut probe = probe();
    probe.tolerances.result.insert("stage".into(), 0.5);
    let engine = run(&probe, synthetic_order(&probe)).unwrap();
    for (field, tolerance) in [
        ("real_seconds", 0.25),
        ("game_seconds", 5.0),
        ("timescale", 0.0),
        ("stage", 0.5),
    ] {
        for (delta, matched) in [(tolerance, true), (tolerance + 0.01, false)] {
            let mut observed = engine.clone();
            match field {
                "real_seconds" => observed[0].real_seconds += delta,
                "game_seconds" => observed[0].game_seconds += delta,
                "timescale" => observed[0].timescale += delta,
                _ => *observed[0].result.get_mut(field).unwrap() += delta,
            }
            assert_eq!(
                compare(&probe, &engine, &observed).unwrap().matched,
                matched,
                "{field} delta {delta}"
            );
        }
    }
}

#[test]
fn tool_04_compare_reports_missing_extra_reordered_and_changed_fields() {
    let probe = probe();
    let engine = run(&probe, synthetic_order(&probe)).unwrap();
    let mut observed = engine.clone();
    observed.remove(1);
    assert!(!compare(&probe, &engine, &observed).unwrap().matched);
    observed = engine.clone();
    observed.push(engine[0].clone());
    assert!(!compare(&probe, &engine, &observed).unwrap().matched);
    observed = engine.clone();
    observed.swap(1, 2);
    assert!(!compare(&probe, &engine, &observed).unwrap().matched);
    for field in [
        "schema",
        "sequence",
        "request_id",
        "phase",
        "skse_version",
        "load_order",
        "forms",
        "paused",
        "result",
    ] {
        let mut changed = serde_json::to_value(&engine).unwrap();
        match field {
            "schema" => changed[0][field] = 2.into(),
            "sequence" | "request_id" => changed[0][field] = "changed".into(),
            "phase" => changed[0][field] = "begin".into(),
            "skse_version" => changed[0][field] = "unexpected SKSE".into(),
            "load_order" => changed[0][field][0] = "changed.esm".into(),
            "forms" => changed[0][field]["runner"] = 123.into(),
            "paused" => changed[0][field] = 1.into(),
            _ => changed[0][field] = serde_json::json!({"unexpected": 1}),
        }
        let observations: Vec<soe_probe::Observation> = serde_json::from_value(changed).unwrap();
        let result = compare(&probe, &engine, &observations);
        assert!(result.is_err() || !result.unwrap().matched, "{field}");
    }
}

#[test]
fn tool_04_compare_retains_runtime_versions_and_rejects_midrun_changes() {
    let probe = probe();
    let engine = run(&probe, synthetic_order(&probe)).unwrap();
    let mut observed = engine.clone();
    for observation in &mut observed {
        observation.runtime_version = "1.6.1170.0".into();
    }
    assert!(compare(&probe, &engine, &observed).unwrap().matched);
    observed[2].runtime_version = "another runtime".into();
    assert!(!compare(&probe, &engine, &observed).unwrap().matched);
}

#[test]
fn tool_04_authoring_rejects_invalid_ids_forms_commands_and_tolerances() {
    let original = probe();
    let mut bad = Vec::new();
    let mut p = original.clone();
    p.steps.push(p.steps[0].clone());
    bad.push(p);
    let mut p = original.clone();
    p.steps = vec![Step::WaitReal {
        id: "bad".into(),
        seconds: f32::NAN,
    }];
    bad.push(p);
    let mut p = original.clone();
    p.tolerances.real_seconds = -1.0;
    bad.push(p);
    let mut p = original.clone();
    p.steps = vec![Step::QuestStage {
        id: "bad".into(),
        quest: "missing".into(),
    }];
    bad.push(p);
    let mut p = original.clone();
    p.steps = vec![Step::FormProperty {
        id: "bad".into(),
        form: "runner".into(),
        property: "name".into(),
    }];
    bad.push(p);
    let mut p = original.clone();
    p.load_order.swap(0, 1);
    bad.push(p);
    let mut p = original.clone();
    p.forms.insert(
        "runner".into(),
        Form {
            plugin: "Skyrim.esm".into(),
            object: 1,
            kind: FormKind::Actor,
        },
    );
    bad.push(p);
    let mut p = original.clone();
    p.timeout_seconds = f64::INFINITY;
    bad.push(p);
    let mut p = original.clone();
    p.tolerances.result.insert("unknown".into(), 1.0);
    bad.push(p);
    for p in bad {
        assert!(matches!(p.validate(), Err(Error::Invalid(_))));
    }
    assert!(Probe::parse(&format!("unknown = true\n{EXAMPLE}")).is_err());
    assert!(Probe::parse(&EXAMPLE.replace("op = \"wait_real\"", "op = \"unknown\"")).is_err());
}

#[test]
fn tool_04_build_rejects_invalid_probe_without_output() {
    let mut probe = probe();
    probe.steps.clear();
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("absent");
    assert!(build(&probe, &out).is_err());
    assert!(!out.exists());
}

#[test]
fn tool_04_run_rejects_missing_mistyped_forms_and_load_order() {
    let mut probe = probe();
    assert!(run(&probe, LoadOrder::new()).is_err());
    probe.forms.insert(
        "actor".into(),
        Form {
            plugin: "Skyrim.esm".into(),
            object: 0x900,
            kind: FormKind::Actor,
        },
    );
    assert!(run(&probe, synthetic_order(&probe))
        .unwrap_err()
        .to_string()
        .contains("missing form actor"));
    probe.forms.get_mut("actor").unwrap().object = 0x3a;
    assert!(run(&probe, synthetic_order(&probe))
        .unwrap_err()
        .to_string()
        .contains("mistyped form actor"));
}

#[test]
fn tool_04_waits_use_real_menu_and_game_clock_domains() {
    for (op, expected) in [("real", 2.5), ("menu", 1.0), ("game", 2.5)] {
        let mut probe = probe();
        probe.menus = vec![
            Menu {
                at: 0.5,
                paused: true,
            },
            Menu {
                at: 2.0,
                paused: false,
            },
        ];
        let id = "wait".into();
        probe.steps = vec![match op {
            "real" => Step::WaitReal { id, seconds: 1.0 },
            "menu" => Step::WaitMenu { id, seconds: 1.0 },
            _ => Step::WaitGame { id, seconds: 20.0 },
        }];
        let observations = run(&probe, synthetic_order(&probe)).unwrap();
        let end = observations.last().unwrap();
        // CONTEXT: Utility's world clock may differ from the VM menu clock.
        if op == "menu" {
            assert_eq!(end.paused, 1);
        } else {
            assert_eq!(end.paused, 0);
            assert!((end.real_seconds - (expected - 1.5)).abs() < 0.02);
        }
        assert_eq!(observations[0].phase, "begin");
    }
}

#[test]
fn tool_04_timeout_reports_unfinished_latent_wait() {
    let mut probe = probe();
    probe.timeout_seconds = 0.5;
    probe.steps = vec![Step::WaitReal {
        id: "wait".into(),
        seconds: 2.0,
    }];
    assert!(run(&probe, synthetic_order(&probe))
        .unwrap_err()
        .to_string()
        .contains("exceeded"));
}

#[test]
fn tool_04_commands_query_forms_actor_values_effects_reset_and_issue_requests() {
    let mut probe = probe();
    for (name, object, kind) in [
        ("actor", 0x100, FormKind::Actor),
        ("effect", 0x200, FormKind::MagicEffect),
        ("spell", 0x300, FormKind::Spell),
    ] {
        probe.forms.insert(
            name.into(),
            Form {
                plugin: "Skyrim.esm".into(),
                object,
                kind,
            },
        );
    }
    probe.steps = vec![
        Step::HasEffect {
            id: "before".into(),
            actor: "actor".into(),
            effect: "effect".into(),
        },
        Step::Cast {
            id: "cast".into(),
            spell: "spell".into(),
            source: "actor".into(),
            target: "actor".into(),
        },
        Step::HasEffect {
            id: "after".into(),
            actor: "actor".into(),
            effect: "effect".into(),
        },
        Step::ActorValue {
            id: "health".into(),
            actor: "actor".into(),
            value: "Health".into(),
        },
        Step::FormProperty {
            id: "form_id".into(),
            form: "actor".into(),
            property: "form_id".into(),
        },
        Step::FormProperty {
            id: "type".into(),
            form: "runner".into(),
            property: "type".into(),
        },
        Step::FormProperty {
            id: "scale".into(),
            form: "timescale".into(),
            property: "global_value".into(),
        },
        Step::SetStage {
            id: "stage".into(),
            quest: "runner".into(),
            stage: 10,
        },
        Step::ResetQuest {
            id: "reset".into(),
            quest: "runner".into(),
        },
        Step::Save { id: "save".into() },
        Step::Animation {
            id: "animation".into(),
            actor: "actor".into(),
            event: "IdleForceDefaultState".into(),
        },
    ];
    let observations = run(&probe, synthetic_order(&probe)).unwrap();
    let results: BTreeMap<_, _> = observations
        .iter()
        .map(|o| (o.request_id.as_str(), &o.result))
        .collect();
    assert_eq!(results["before"]["value"], 0.0);
    assert_eq!(results["after"]["value"], 1.0);
    assert_eq!(results["health"]["value"], 110.0);
    assert_eq!(results["form_id"]["value"], 256.0);
    assert_eq!(results["scale"]["value"], 20.0);
    assert_eq!(results["reset"]["stage"], 0.0);
    assert_eq!(results["save"]["issued"], 1.0);
    assert_eq!(results["animation"]["issued"], 1.0);
}

#[test]
fn tool_04_cli_build_parse_and_missing_data_have_expected_exit_status() {
    let exe = env!("CARGO_BIN_EXE_soe-probe");
    let dir = tempfile::tempdir().unwrap();
    assert!(Command::new(exe)
        .arg("build")
        .arg(dir.path())
        .status()
        .unwrap()
        .success());
    let log = dir.path().join("Papyrus.0.log");
    std::fs::write(&log, LOG).unwrap();
    let output = Command::new(exe).arg("parse").arg(&log).output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Vec<soe_probe::Observation>>(&output.stdout)
            .unwrap()
            .len(),
        7
    );
    let authored = dir.path().join("probe.toml");
    std::fs::write(&authored, EXAMPLE).unwrap();
    let output = Command::new(exe)
        .arg("compare")
        .arg(&authored)
        .arg(&log)
        .env_remove("SKYRIM_DATA")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("requires SKYRIM_DATA"));
}

#[test]
fn tool_04_installed_masters_run_the_authored_sequence_and_cli_compare() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let probe = probe();
    let engine = run_installed(&probe, std::path::Path::new(&data)).unwrap();
    assert_eq!(engine.last().unwrap().result["stage"], 20.0);
    assert_eq!(engine[0].forms["timescale"], 0x3a);
    let dir = tempfile::tempdir().unwrap();
    let authored = dir.path().join("probe.toml");
    let observed = dir.path().join("observations.json");
    std::fs::write(&authored, EXAMPLE).unwrap();
    std::fs::write(&observed, serde_json::to_vec(&engine).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-probe"))
        .arg("compare")
        .arg(&authored)
        .arg(&observed)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["matched"], true);
    let mut changed = engine;
    changed[0].result.insert("stage".into(), 999.0);
    std::fs::write(&observed, serde_json::to_vec(&changed).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_soe-probe"))
        .arg("compare")
        .arg(&authored)
        .arg(&observed)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn tool_04_script_and_plugin_builders_reject_unvalidated_input() {
    let mut probe = probe();
    probe.name = "invalid\"json".into();
    assert!(script(&probe).is_err());
    assert!(plugin(&probe).is_err());
}

#[test]
fn tool_04_aliases_follow_papyrus_case_insensitive_identity() {
    let mut probe = probe();
    probe.forms.insert(
        "RUNNER".into(),
        Form {
            plugin: "SoeProbe.esp".into(),
            object: 0x800,
            kind: FormKind::Quest,
        },
    );
    assert!(probe.validate().is_err());
    probe.forms.remove("RUNNER");
    probe
        .forms
        .insert("ANCHOR_UPDATE".into(), probe.forms["anchor_update"].clone());
    assert!(probe.validate().is_err());
}

#[test]
fn tool_04_animation_output_wait_reports_missing_host_event() {
    let mut probe = probe();
    probe.timeout_seconds = 0.5;
    probe.forms.insert(
        "actor".into(),
        Form {
            plugin: "Skyrim.esm".into(),
            object: 0x100,
            kind: FormKind::Actor,
        },
    );
    probe.steps = vec![Step::WaitAnimation {
        id: "event".into(),
        actor: "actor".into(),
        event: "FootLeft".into(),
    }];
    assert!(run(&probe, synthetic_order(&probe))
        .unwrap_err()
        .to_string()
        .contains("exceeded"));
}

#[test]
fn tool_04_compare_rejects_changing_engine_version() {
    let probe = probe();
    let mut engine = run(&probe, synthetic_order(&probe)).unwrap();
    let observed = engine.clone();
    engine[1].runtime_version = "another engine".into();
    assert!(compare(&probe, &engine, &observed)
        .unwrap_err()
        .to_string()
        .contains("version changed"));
}

#[test]
fn tool_04_compare_exposes_real_clock_disagreement_during_menu_wait() {
    let mut probe = probe();
    probe.menus = vec![Menu {
        at: 0.5,
        paused: true,
    }];
    probe.steps = vec![Step::WaitMenu {
        id: "menu_wait".into(),
        seconds: 1.0,
    }];
    let engine = run(&probe, synthetic_order(&probe)).unwrap();
    let mut vanilla_model = engine.clone();
    vanilla_model[1].real_seconds = 1.0;
    let report = compare(&probe, &engine, &vanilla_model).unwrap();
    assert!(report
        .differences
        .iter()
        .any(|difference| difference.field == "real_seconds"));
}
