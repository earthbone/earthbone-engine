// SPDX-License-Identifier: GPL-3.0-or-later

//! Skyrim PEX 3.2 and QUST output, using asm, quest and VMAD builders.
//! Layout references: Caprica PexFile/PexDebugInfo, Champollion FileReader,
//! TES5Edit wbDefinitionsTES5 QUST and xeMainForm GenerateSEQFileForFile.

use std::{collections::BTreeSet, path::Path};

use soe_esm::{FileHeader, Plugin, Tag};
use soe_game::{
    quest,
    vmad::{ScriptEntry, Vmad},
};
use soe_papyrus::{
    asm::{self, b, f, i, ident, none, s, FunctionBuilder, ScriptBuilder},
    pex::{Function, Instruction, Opcode, Operand, Script},
};

use crate::{Error, FormKind, Probe, Step};

pub(crate) const SCRIPT: &str = "SoeProbeRunner";
pub(crate) const QUEST_ID: u32 = 0x05000800;
const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

fn ins(fun: &mut Function, op: Opcode, args: Vec<Operand>) {
    fun.code.push(Instruction { op, args });
}

fn global(fun: &mut Function, class: &str, name: &str, dst: &str, args: Vec<Operand>) {
    let mut operands = vec![ident(class), ident(name), ident(dst)];
    operands.extend(args);
    ins(fun, Opcode::CallStatic, operands);
}

fn method(fun: &mut Function, object: &str, name: &str, dst: &str, args: Vec<Operand>) {
    let mut operands = vec![ident(name), ident(object), ident(dst)];
    operands.extend(args);
    ins(fun, Opcode::CallMethod, operands);
}

fn cat(fun: &mut Function, value: Operand) {
    ins(
        fun,
        Opcode::StrCat,
        vec![ident("json"), ident("json"), value],
    );
}

fn number(fun: &mut Function, name: &str) {
    ins(fun, Opcode::Cast, vec![ident("text"), ident(name)]);
    cat(fun, ident("text"));
}

fn observe(probe: &Probe) -> Result<Function, Error> {
    let mut fun = FunctionBuilder::new("Observe", "None")
        .param("request", "String")
        .param("phase", "String")
        .param("result", "String")
        .local("json", "String")
        .local("text", "String")
        .local("version", "String")
        .local("real", "Float")
        .local("game", "Float")
        .local("scale", "Float")
        .local("paused", "Bool")
        .local("pauseInt", "Int")
        .local("formId", "Int")
        .local("discard", "None")
        .build();
    global(&mut fun, "Debug", "GetVersionNumber", "version", vec![]);
    global(&mut fun, "Utility", "GetCurrentRealTime", "real", vec![]);
    global(&mut fun, "Utility", "GetCurrentGameTime", "game", vec![]);
    global(&mut fun, "Utility", "IsInMenuMode", "paused", vec![]);
    method(&mut fun, "f_timescale", "GetValue", "scale", vec![]);
    ins(
        &mut fun,
        Opcode::FSub,
        vec![ident("real"), ident("real"), ident("startReal")],
    );
    ins(
        &mut fun,
        Opcode::FSub,
        vec![ident("game"), ident("game"), ident("startGame")],
    );
    ins(
        &mut fun,
        Opcode::FMul,
        vec![ident("game"), ident("game"), f(86400.0)],
    );
    ins(
        &mut fun,
        Opcode::Cast,
        vec![ident("pauseInt"), ident("paused")],
    );
    ins(
        &mut fun,
        Opcode::Assign,
        vec![
            ident("json"),
            s(&format!(
                "SOEPROBE {{\"schema\":1,\"sequence\":\"{}\",\"request_id\":\"",
                probe.name
            )),
        ],
    );
    cat(&mut fun, ident("request"));
    cat(&mut fun, s("\",\"phase\":\""));
    cat(&mut fun, ident("phase"));
    cat(&mut fun, s("\",\"runtime_version\":\""));
    cat(&mut fun, ident("version"));
    cat(
        &mut fun,
        s(&format!(
            "\",\"skse_version\":null,\"load_order\":{},\"forms\":{{",
            serde_json::to_string(&probe.load_order)?
        )),
    );
    for (index, name) in probe.all_forms().keys().enumerate() {
        cat(
            &mut fun,
            s(&format!("{}\"{name}\":", if index == 0 { "" } else { "," })),
        );
        method(
            &mut fun,
            &format!("f_{name}"),
            "GetFormID",
            "formId",
            vec![],
        );
        number(&mut fun, "formId");
    }
    cat(&mut fun, s("},\"timescale\":"));
    number(&mut fun, "scale");
    cat(&mut fun, s(",\"real_seconds\":"));
    number(&mut fun, "real");
    cat(&mut fun, s(",\"game_seconds\":"));
    number(&mut fun, "game");
    cat(&mut fun, s(",\"paused\":"));
    number(&mut fun, "pauseInt");
    cat(&mut fun, s(",\"result\":"));
    cat(&mut fun, ident("result"));
    cat(&mut fun, s("}"));
    global(
        &mut fun,
        "Debug",
        "Trace",
        "discard",
        vec![ident("json"), i(0)],
    );
    ins(&mut fun, Opcode::Return, vec![none()]);
    Ok(fun)
}

fn result_number(fun: &mut Function, key: &str, variable: &str) {
    ins(
        fun,
        Opcode::Assign,
        vec![ident("json"), s(&format!("{{\"{key}\":"))],
    );
    number(fun, variable);
    cat(fun, s("}"));
}

fn observation(fun: &mut Function, step: &Step, phase: &str, result: Operand) {
    method(
        fun,
        "self",
        "Observe",
        "discard",
        vec![s(step.id()), s(phase), result],
    );
}

fn step_function(step: &Step, index: usize) -> Function {
    let mut fun = FunctionBuilder::new(&format!("Step{index}"), "None")
        .local("discard", "None")
        .local("flag", "Bool")
        .local("integer", "Int")
        .local("stage", "Int")
        .local("value", "Float")
        .local("json", "String")
        .local("text", "String")
        .build();
    ins(
        &mut fun,
        Opcode::Assign,
        vec![ident("json"), s("{\"issued\":1}")],
    );
    if step.waits() {
        observation(&mut fun, step, "begin", s("{}"));
    }
    match step {
        Step::SetStage { quest, stage, .. } => {
            method(
                &mut fun,
                &format!("f_{quest}"),
                "SetCurrentStageID",
                "flag",
                vec![i(i32::from(*stage))],
            );
            method(
                &mut fun,
                &format!("f_{quest}"),
                "GetCurrentStageID",
                "stage",
                vec![],
            );
            ins(
                &mut fun,
                Opcode::Cast,
                vec![ident("integer"), ident("flag")],
            );
            ins(
                &mut fun,
                Opcode::Assign,
                vec![ident("json"), s("{\"accepted\":")],
            );
            number(&mut fun, "integer");
            cat(&mut fun, s(",\"stage\":"));
            number(&mut fun, "stage");
            cat(&mut fun, s("}"));
        }
        Step::ResetQuest { quest, .. } | Step::QuestStage { quest, .. } => {
            if matches!(step, Step::ResetQuest { .. }) {
                method(&mut fun, &format!("f_{quest}"), "Reset", "discard", vec![]);
            }
            method(
                &mut fun,
                &format!("f_{quest}"),
                "GetCurrentStageID",
                "stage",
                vec![],
            );
            result_number(&mut fun, "stage", "stage");
        }
        Step::WaitReal { seconds, .. }
        | Step::WaitMenu { seconds, .. }
        | Step::WaitGame { seconds, .. } => {
            let (name, duration) = match step {
                Step::WaitReal { .. } => ("Wait", *seconds),
                Step::WaitMenu { .. } => ("WaitMenuMode", *seconds),
                _ => ("WaitGameTime", *seconds / 3600.0),
            };
            global(&mut fun, "Utility", name, "discard", vec![f(duration)]);
        }
        Step::Animation { actor, event, .. } => global(
            &mut fun,
            "Debug",
            "SendAnimationEvent",
            "discard",
            vec![ident(&format!("f_{actor}")), s(event)],
        ),
        Step::AnimationEvents {
            actor,
            events,
            set_int,
            ..
        } => {
            for (variable, value) in set_int {
                method(
                    &mut fun,
                    &format!("f_{actor}"),
                    "SetAnimationVariableInt",
                    "discard",
                    vec![s(variable), i(*value)],
                );
            }
            for event in events {
                global(
                    &mut fun,
                    "Debug",
                    "SendAnimationEvent",
                    "discard",
                    vec![ident(&format!("f_{actor}")), s(event)],
                );
            }
        }
        Step::AnimationInt {
            actor,
            variable,
            set,
            ..
        } => {
            let object = format!("f_{actor}");
            if let Some(value) = set {
                method(
                    &mut fun,
                    &object,
                    "SetAnimationVariableInt",
                    "discard",
                    vec![s(variable), i(*value)],
                );
            }
            method(
                &mut fun,
                &object,
                "GetAnimationVariableInt",
                "integer",
                vec![s(variable)],
            );
            result_number(&mut fun, "value", "integer");
        }
        Step::AnimationFloat {
            actor,
            variable,
            set,
            ..
        } => {
            let object = format!("f_{actor}");
            if let Some(value) = set {
                method(
                    &mut fun,
                    &object,
                    "SetAnimationVariableFloat",
                    "discard",
                    vec![s(variable), f(*value)],
                );
            }
            method(
                &mut fun,
                &object,
                "GetAnimationVariableFloat",
                "value",
                vec![s(variable)],
            );
            result_number(&mut fun, "value", "value");
        }
        Step::EquipItem { actor, item, .. } => {
            let object = format!("f_{actor}");
            method(
                &mut fun,
                &object,
                "AddItem",
                "discard",
                vec![ident(&format!("f_{item}")), i(1), b(true)],
            );
            method(
                &mut fun,
                &object,
                "EquipItem",
                "discard",
                vec![ident(&format!("f_{item}")), b(false), b(true)],
            );
        }
        Step::ThirdPerson { .. } => global(&mut fun, "Game", "ForceThirdPerson", "discard", vec![]),
        Step::WaitAnimation { actor, event, .. } => {
            method(
                &mut fun,
                &format!("f_{actor}"),
                "WaitForAnimationEvent",
                "flag",
                vec![s(event)],
            );
            ins(
                &mut fun,
                Opcode::Cast,
                vec![ident("integer"), ident("flag")],
            );
            result_number(&mut fun, "value", "integer");
        }
        Step::Cast {
            spell,
            source,
            target,
            ..
        } => method(
            &mut fun,
            &format!("f_{spell}"),
            "Cast",
            "discard",
            vec![ident(&format!("f_{source}")), ident(&format!("f_{target}"))],
        ),
        Step::ActorValue { actor, value, .. } => {
            method(
                &mut fun,
                &format!("f_{actor}"),
                "GetActorValue",
                "value",
                vec![s(value)],
            );
            result_number(&mut fun, "value", "value");
        }
        Step::HasEffect { actor, effect, .. } => {
            method(
                &mut fun,
                &format!("f_{actor}"),
                "HasMagicEffect",
                "flag",
                vec![ident(&format!("f_{effect}"))],
            );
            ins(
                &mut fun,
                Opcode::Cast,
                vec![ident("integer"), ident("flag")],
            );
            result_number(&mut fun, "value", "integer");
        }
        Step::FormProperty { form, property, .. } => {
            let float = property == "global_value";
            let name = match property.as_str() {
                "type" => "GetType",
                "form_id" => "GetFormID",
                _ => "GetValue",
            };
            let dst = if float { "value" } else { "integer" };
            method(&mut fun, &format!("f_{form}"), name, dst, vec![]);
            result_number(&mut fun, "value", dst);
        }
        Step::Save { .. } => global(&mut fun, "Game", "RequestSave", "discard", vec![]),
    }
    observation(&mut fun, step, "end", ident("json"));
    ins(&mut fun, Opcode::Return, vec![none()]);
    fun
}

/// Build the runner PEX model with vanilla native calls and Skyrim metadata.
///
/// # Errors
/// Rejects invalid sequences and unrepresentable output.
pub fn script(probe: &Probe) -> Result<Script, Error> {
    probe.validate()?;
    let mut builder = ScriptBuilder::new(SCRIPT, "Quest")
        .variable("started", "Bool", b(false))
        .variable("startReal", "Float", f(0.0))
        .variable("startGame", "Float", f(0.0));
    let mut run = FunctionBuilder::new("RunSequence", "None")
        .local("raw", "Form")
        .local("missing", "Bool")
        .local("discard", "None")
        .build();
    ins(&mut run, Opcode::JmpF, vec![ident("started"), i(2)]);
    ins(&mut run, Opcode::Return, vec![none()]);
    ins(&mut run, Opcode::Assign, vec![ident("started"), b(true)]);
    for (name, form) in probe.all_forms() {
        let var = format!("f_{name}");
        builder = builder.variable(&var, form.kind.papyrus(), none());
        if name == "runner" {
            ins(&mut run, Opcode::Assign, vec![ident(&var), ident("self")]);
        } else if form.kind == FormKind::Player {
            global(&mut run, "Game", "GetPlayer", &var, vec![]);
        } else {
            global(
                &mut run,
                "Game",
                "GetFormFromFile",
                "raw",
                vec![i(form.object as i32), s(&form.plugin)],
            );
            ins(&mut run, Opcode::Cast, vec![ident(&var), ident("raw")]);
        }
        ins(
            &mut run,
            Opcode::CmpEq,
            vec![ident("missing"), ident(&var), none()],
        );
        ins(&mut run, Opcode::JmpF, vec![ident("missing"), i(3)]);
        global(
            &mut run,
            "Debug",
            "Trace",
            "discard",
            vec![
                s(&format!("SoeProbe missing or mistyped form {name}")),
                i(2),
            ],
        );
        ins(&mut run, Opcode::Return, vec![none()]);
    }
    if probe.startup_wait_seconds > 0.0 {
        global(
            &mut run,
            "Utility",
            "Wait",
            "discard",
            vec![f(probe.startup_wait_seconds)],
        );
    }
    global(
        &mut run,
        "Utility",
        "GetCurrentRealTime",
        "startReal",
        vec![],
    );
    global(
        &mut run,
        "Utility",
        "GetCurrentGameTime",
        "startGame",
        vec![],
    );
    for (index, step) in probe.steps.iter().enumerate() {
        method(&mut run, "self", &format!("Step{index}"), "discard", vec![]);
        builder = builder.function(step_function(step, index));
    }
    ins(&mut run, Opcode::Return, vec![none()]);
    let init = FunctionBuilder::new("OnInit", "None")
        .local("discard", "None")
        .ins(
            Opcode::CallMethod,
            vec![ident("RunSequence"), ident("self"), ident("discard")],
        )
        .ins(Opcode::Return, vec![none()])
        .build();
    let mut script = builder
        .function(init)
        .function(run)
        .function(observe(probe)?)
        .build();
    let state_helpers = soe_papyrus::psc::compile(
        "SoeProbeRunner.psc",
        "Scriptname SoeProbeRunner extends Quest Hidden",
    )
    .map_err(|e| Error::Execution(e.to_string()))?;
    let helpers =
        soe_papyrus::pex::parse(&state_helpers).map_err(|e| Error::Execution(e.to_string()))?;
    script.objects[0].states[0]
        .functions
        .extend(helpers.objects[0].states[0].functions.iter().cloned());
    script.objects[0].user_flags = 1;
    for state in &mut script.objects[0].states {
        for fun in &mut state.functions {
            fun.lines = vec![1; fun.code.len()];
        }
    }
    Ok(script)
}

/// Build a new quest plugin with five official masters and a VMAD runner attachment.
///
/// # Errors
/// Rejects invalid sequences before building records.
pub fn plugin(probe: &Probe) -> Result<Plugin, Error> {
    probe.validate()?;
    let mut plugin = Plugin::new(FileHeader {
        author: "soe".into(),
        description: format!("Authored runtime probe {}", probe.name),
        masters: MASTERS.iter().map(|s| (*s).into()).collect(),
        record_count: 2,
        next_object_id: 0x801,
        ..FileHeader::default()
    });
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new(SCRIPT)],
        ..Vmad::default()
    };
    let mut quest = quest::build_record(
        QUEST_ID,
        "SoeProbe",
        quest::flags::START_GAME_ENABLED | quest::flags::RUN_ONCE,
    );
    // CONTEXT: QUST ordering is significant, VMAD precedes DNAM in xEdit's schema.
    quest.subrecords.insert(
        1,
        soe_esm::Subrecord::new(Tag(*b"VMAD"), vmad.write(Tag(*b"QUST"))),
    );
    quest = quest.zsub(Tag(*b"FLTR"), "SoeProbe").sub(Tag(*b"NEXT"), []);
    let mut stages = BTreeSet::from([0]);
    for step in &probe.steps {
        if let Step::SetStage { quest, stage, .. } = step {
            if quest == "runner" {
                stages.insert(*stage);
            }
        }
    }
    for stage in stages {
        quest = quest::with_stage(quest, stage, 0, 0, "");
    }
    quest = quest.sub(Tag(*b"ANAM"), 0u32.to_le_bytes());
    plugin.push_top(quest);
    Ok(plugin)
}

/// Write an authored plugin, runner and startup sequence file into a Data-shaped folder.
///
/// # Errors
/// Rejects invalid sequences or reports filesystem failures.
pub fn build(probe: &Probe, out: &Path) -> Result<(), Error> {
    let pex = asm::write(&script(probe)?);
    let esp = plugin(probe)?.to_bytes();
    std::fs::create_dir_all(out.join("Scripts"))?;
    std::fs::create_dir_all(out.join("Seq"))?;
    std::fs::write(out.join("SoeProbe.esp"), esp)?;
    std::fs::write(out.join("Scripts/SoeProbeRunner.pex"), pex)?;
    std::fs::write(out.join("Seq/SoeProbe.seq"), QUEST_ID.to_le_bytes())?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod anm_56 {
    use super::{script, Opcode, Operand, Probe};

    #[test]
    fn anm_56_startup_wait_precedes_recorded_clock_samples() {
        let mut probe = Probe::parse(crate::EXAMPLE).unwrap();
        probe.startup_wait_seconds = 3.0;
        let script = script(&probe).unwrap();
        let run = script.objects[0].function_in("", "RunSequence").unwrap();
        let native_index = |name: &str| {
            run.code
                .iter()
                .position(|instruction| {
                    instruction.op == Opcode::CallStatic
                        && instruction.args[1] == Operand::Identifier(name.into())
                })
                .unwrap()
        };
        let wait = native_index("Wait");
        assert!(wait < native_index("GetCurrentRealTime"));
        assert!(wait < native_index("GetCurrentGameTime"));
        assert_eq!(run.code[wait].args[3], Operand::Float(3.0));
    }
}
