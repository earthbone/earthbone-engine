// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use soe_papyrus::pex::{self, Function, Script};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionShape {
    return_type: String,
    params: Vec<String>,
    instructions: usize,
}

pub type FunctionKey = (String, String, String, String);

pub fn pex_functions(script: &Script) -> BTreeMap<FunctionKey, FunctionShape> {
    let mut functions = BTreeMap::new();
    for object in &script.objects {
        for state in &object.states {
            for function in &state.functions {
                add_function(
                    &mut functions,
                    &object.name,
                    &state.name,
                    &function.name,
                    "",
                    function,
                );
            }
        }
        for property in &object.properties {
            if let Some(function) = &property.getter {
                add_function(
                    &mut functions,
                    &object.name,
                    "",
                    "get",
                    &property.name,
                    function,
                );
            }
            if let Some(function) = &property.setter {
                add_function(
                    &mut functions,
                    &object.name,
                    "",
                    "set",
                    &property.name,
                    function,
                );
            }
        }
    }
    functions
}

fn add_function(
    functions: &mut BTreeMap<FunctionKey, FunctionShape>,
    object: &str,
    state: &str,
    name: &str,
    property: &str,
    function: &Function,
) {
    functions.insert(
        (
            object.to_ascii_lowercase(),
            state.to_ascii_lowercase(),
            name.to_ascii_lowercase(),
            property.to_ascii_lowercase(),
        ),
        FunctionShape {
            return_type: function.return_type.to_ascii_lowercase(),
            params: function
                .params
                .iter()
                .map(|param| {
                    format!(
                        "{}:{}",
                        param.name.to_ascii_lowercase(),
                        param.type_name.to_ascii_lowercase()
                    )
                })
                .collect(),
            instructions: function.code.len(),
        },
    );
}

pub fn champollion_functions(assembly: &str) -> BTreeMap<FunctionKey, FunctionShape> {
    let mut result = BTreeMap::new();
    let (mut object, mut state, mut function, mut property) =
        (String::new(), String::new(), String::new(), String::new());
    let (mut return_type, mut params, mut instructions) = (String::new(), Vec::new(), 0usize);
    let mut in_code = false;

    for line in assembly.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(".object ") {
            object = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
        } else if let Some(rest) = line.strip_prefix(".property ") {
            property = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
        } else if line == ".endProperty" {
            property.clear();
        } else if line == ".state" {
            state.clear();
        } else if let Some(rest) = line.strip_prefix(".state ") {
            state = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
        } else if let Some(rest) = line.strip_prefix(".function ") {
            function = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned();
            params.clear();
            instructions = 0;
            in_code = false;
            return_type.clear();
        } else if let Some(rest) = line.strip_prefix(".return ") {
            return_type = rest
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
        } else if let Some(rest) = line.strip_prefix(".param ") {
            let mut fields = rest.split_whitespace();
            if let (Some(param_name), Some(param_type)) = (fields.next(), fields.next()) {
                params.push(format!(
                    "{}:{}",
                    param_name.to_ascii_lowercase(),
                    param_type.to_ascii_lowercase()
                ));
            }
        } else if line == ".code" {
            in_code = true;
        } else if line == ".endCode" {
            in_code = false;
        } else if let Some(_rest) = line.strip_prefix(".endFunction") {
            if !function.is_empty() {
                result.insert(
                    (
                        object.to_ascii_lowercase(),
                        state.to_ascii_lowercase(),
                        function.to_ascii_lowercase(),
                        property.to_ascii_lowercase(),
                    ),
                    FunctionShape {
                        return_type: return_type.clone(),
                        params: params.clone(),
                        instructions,
                    },
                );
            }
            function.clear();
        } else if in_code && !line.is_empty() && !line.starts_with(';') && !line.starts_with('_') {
            instructions += 1;
        }
    }
    result
}

pub fn differences(
    source: &BTreeMap<FunctionKey, FunctionShape>,
    decoded: &BTreeMap<FunctionKey, FunctionShape>,
) -> Vec<String> {
    let keys: std::collections::BTreeSet<_> = source.keys().chain(decoded.keys()).collect();
    keys.into_iter()
        .filter_map(|key| match (source.get(key), decoded.get(key)) {
            (Some(left), Some(right)) if left == right => None,
            (left, right) => Some(format!(
                "{}::{}::{}::{} PEX={left:?} Champollion={right:?}",
                key.0, key.1, key.2, key.3
            )),
        })
        .collect()
}

fn scratch_dir() -> PathBuf {
    std::env::temp_dir().join(format!("soe-tst-10-champollion-{}", std::process::id()))
}

fn corpus_inputs(
    data: Option<OsString>,
    champollion: Option<OsString>,
) -> Result<Option<(PathBuf, PathBuf)>, &'static str> {
    let Some(data) = data else {
        return Ok(None);
    };
    let Some(champollion) = champollion else {
        return Err("CHAMPOLLION must name the decoder when SKYRIM_DATA is set");
    };
    Ok(Some((PathBuf::from(data), PathBuf::from(champollion))))
}

fn compare_corpus(data: &Path, champollion: &Path, report: &Path) {
    let archive =
        soe_bsa::Archive::open(&data.join("Skyrim - Misc.bsa")).expect("open Skyrim - Misc.bsa");
    let names: Vec<_> = archive
        .files()
        .iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".pex"))
        .cloned()
        .collect();
    assert!(
        !names.is_empty(),
        "the scripts archive contains no PEX files"
    );

    let root = scratch_dir();
    let input = root.join("input");
    let assembly_dir = root.join("assembly");
    let psc_dir = root.join("psc");
    fs::create_dir_all(&input).expect("create input directory");
    fs::create_dir_all(&assembly_dir).expect("create assembly directory");
    fs::create_dir_all(&psc_dir).expect("create PSC directory");

    let mut expected = BTreeMap::new();
    let mut inputs = Vec::with_capacity(names.len());
    for (index, name) in names.iter().enumerate() {
        let bytes = archive
            .read(name)
            .unwrap_or_else(|error| panic!("read {name}: {error}"));
        let parsed = pex::parse(&bytes).unwrap_or_else(|error| panic!("parse {name}: {error}"));
        expected.insert(index, (name.clone(), pex_functions(&parsed)));
        let path = input.join(format!("{index:05}.pex"));
        fs::write(&path, bytes).expect("write PEX scratch file");
        inputs.push(path);
    }

    let mut command = Command::new(champollion);
    command.arg("-a").arg(&assembly_dir).arg("-p").arg(&psc_dir);
    command.args(&inputs);
    let output = command.output().expect("run Champollion");
    assert!(
        output.status.success(),
        "Champollion failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let mut report_lines = Vec::new();
    let mut differing_scripts = 0usize;
    let mut compared = 0usize;
    for (index, (name, expected_functions)) in expected {
        let assembly_path = assembly_dir.join(format!("{index:05}.pas"));
        let assembly = fs::read_to_string(&assembly_path)
            .unwrap_or_else(|error| panic!("Champollion output missing for {name}: {error}"));
        let actual_functions = champollion_functions(&assembly);
        let diff = differences(&expected_functions, &actual_functions);
        if !diff.is_empty() {
            differing_scripts += 1;
            report_lines.push(format!("{name}:"));
            report_lines.extend(diff.into_iter().map(|line| format!("  {line}")));
        }
        compared += 1;
    }
    fs::write(
        report,
        report_lines.join("\n") + if report_lines.is_empty() { "" } else { "\n" },
    )
    .expect("write differential report");
    fs::remove_dir_all(&root).expect("remove Champollion scratch directory");
    eprintln!(
        "TST-10 compared {compared} scripts. Differences found in {differing_scripts} scripts, listed in {}",
        report.display()
    );
}

#[test]
fn tst_10_champollion_assembly_inventory_parser_reads_signatures_and_opcodes() {
    let assembly = ".object Example Quest\n .state Running\n  .function Tick\n   .return Int\n   .param amount int\n   .code\n    IADD x y z\n    RETURN x\n   .endCode\n  .endFunction ;Tick\n .endState\n.endObject\n";
    let functions = champollion_functions(assembly);
    assert_eq!(
        functions.get(&(
            "example".into(),
            "running".into(),
            "tick".into(),
            String::new(),
        )),
        Some(&FunctionShape {
            return_type: "int".into(),
            params: vec!["amount:int".into()],
            instructions: 2,
        })
    );
}

#[test]
fn tst_10_champollion_requires_decoder_when_game_data_is_configured() {
    assert_eq!(corpus_inputs(None, None), Ok(None));
    assert_eq!(
        corpus_inputs(Some(OsString::from("Data")), None),
        Err("CHAMPOLLION must name the decoder when SKYRIM_DATA is set")
    );
    assert_eq!(
        corpus_inputs(
            Some(OsString::from("Data")),
            Some(OsString::from("champollion"))
        ),
        Ok(Some((PathBuf::from("Data"), PathBuf::from("champollion"))))
    );
}

#[test]
fn tst_10_champollion_reports_unmatched_loaded_functions() {
    let pex = BTreeMap::from([(
        (
            "example".into(),
            String::new(),
            "tick".into(),
            String::new(),
        ),
        FunctionShape {
            return_type: "int".into(),
            params: vec!["int:int".into()],
            instructions: 2,
        },
    )]);
    let decoded = BTreeMap::new();
    assert_eq!(
        differences(&pex, &decoded),
        vec!["example::::tick:: PEX=Some(FunctionShape { return_type: \"int\", params: [\"int:int\"], instructions: 2 }) Champollion=None"]
    );
}

#[test]
fn tst_10_champollion_equal_function_inventories_have_no_differences() {
    let loaded = BTreeMap::from([(
        (
            "example".into(),
            String::new(),
            "tick".into(),
            String::new(),
        ),
        FunctionShape {
            return_type: "int".into(),
            params: vec!["int:int".into()],
            instructions: 2,
        },
    )]);

    assert!(
        differences(&loaded, &loaded).is_empty(),
        "matching function inventories produce an empty report"
    );
}

#[test]
fn tst_10_champollion_instruction_inventory_skips_labels_and_debug_comments() {
    let assembly = ".object Example\n.state\n.function Tick\n.return Int\n.param value Int\n.code\n; line 12\nJMP _label0\n_label0:\nRETURN value\n.endCode\n.endFunction ;Tick\n.endState\n.endObject\n";
    let functions = champollion_functions(assembly);
    assert_eq!(
        functions.get(&(
            "example".into(),
            String::new(),
            "tick".into(),
            String::new(),
        )),
        Some(&FunctionShape {
            return_type: "int".into(),
            params: vec!["value:int".into()],
            instructions: 2,
        }),
        "labels and debug comments are not instructions"
    );
}

#[test]
fn tst_10_champollion_report_differences_include_missing_and_changed_functions() {
    let source = BTreeMap::from([
        (
            (
                "example".into(),
                "running".into(),
                "tick".into(),
                String::new(),
            ),
            FunctionShape {
                return_type: "int".into(),
                params: vec!["int:int".into()],
                instructions: 2,
            },
        ),
        (
            (
                "example".into(),
                String::new(),
                "get".into(),
                "health".into(),
            ),
            FunctionShape {
                return_type: "none".into(),
                params: Vec::new(),
                instructions: 1,
            },
        ),
    ]);
    let champollion = BTreeMap::from([(
        (
            "example".into(),
            "running".into(),
            "tick".into(),
            String::new(),
        ),
        FunctionShape {
            return_type: "string".into(),
            params: vec!["string:string".into()],
            instructions: 3,
        },
    )]);

    assert_eq!(
        differences(&source, &champollion),
        vec![
            "example::::get::health PEX=Some(FunctionShape { return_type: \"none\", params: [], instructions: 1 }) Champollion=None".to_owned(),
            "example::running::tick:: PEX=Some(FunctionShape { return_type: \"int\", params: [\"int:int\"], instructions: 2 }) Champollion=Some(FunctionShape { return_type: \"string\", params: [\"string:string\"], instructions: 3 })".to_owned(),
        ],
        "the report includes missing functions and signature or instruction differences"
    );
}

#[test]
fn tst_10_champollion_matches_all_shipped_scripts_or_writes_differences() {
    let inputs = corpus_inputs(
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("CHAMPOLLION"),
    )
    .unwrap_or_else(|message| panic!("TST-10 configuration error: {message}"));
    let Some((data, champollion)) = inputs else {
        eprintln!("skipping TST-10: SKYRIM_DATA is not set");
        return;
    };
    let default_report = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target"))
        .join("tst-10-champollion-differences.txt");
    let report = std::env::var_os("SOE_TST_10_REPORT")
        .map(PathBuf::from)
        .unwrap_or(default_report);
    compare_corpus(&data, &champollion, &report);
}
