// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Compare a normalized xEdit record dump with Skyrim.esm.
//!
//! The input is tab-separated. Each record starts with
//! `RECORD<TAB>signature<TAB>form-id`, followed by zero or more
//! `FIELD<TAB>signature<TAB>form-id<TAB>subrecord<TAB>ordinal<TAB>hex-bytes`
//! lines. Form IDs and payload bytes are hexadecimal. Ordinals start at zero
//! for repeated subrecords. The normalization retains xEdit's record and
//! subrecord identities and byte values while omitting its presentation tree.
//!
//! Set `SOE_XEDIT_DUMP` to this dump and `SKYRIM_DATA` to the installed Data
//! directory to run the live comparison.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::PathBuf;

use soe_esm::{Plugin, Tag};

type RecordKey = (String, u32);
type FieldKey = (RecordKey, String, usize);

const XEDIT_RECORD_SIGNATURES: [&str; 6] = ["QUST", "NPC_", "PACK", "CELL", "REFR", "LVLI"];

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Dump {
    records: BTreeMap<RecordKey, usize>,
    fields: BTreeMap<FieldKey, Vec<u8>>,
}

pub(crate) fn parse_dump(input: &str) -> Result<Dump, String> {
    let mut dump = Dump::default();
    for (line_index, line) in input.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.split('\t').collect();
        let line_number = line_index + 1;
        match parts.as_slice() {
            ["RECORD", signature, form_id] => {
                validate_signature(signature).map_err(|e| format!("line {line_number}: {e}"))?;
                let form_id =
                    parse_hex_u32(form_id).map_err(|e| format!("line {line_number}: {e}"))?;
                let key = (signature.to_string(), form_id);
                if dump.records.insert(key.clone(), 1).is_some() {
                    return Err(format!(
                        "line {line_number}: duplicate record {signature}:{form_id:08X}"
                    ));
                }
            }
            ["FIELD", signature, form_id, subrecord, ordinal, bytes] => {
                validate_signature(signature).map_err(|e| format!("line {line_number}: {e}"))?;
                validate_signature(subrecord).map_err(|e| format!("line {line_number}: {e}"))?;
                let form_id =
                    parse_hex_u32(form_id).map_err(|e| format!("line {line_number}: {e}"))?;
                let ordinal = ordinal
                    .parse::<usize>()
                    .map_err(|_| format!("line {line_number}: invalid ordinal {ordinal:?}"))?;
                let data =
                    parse_hex_bytes(bytes).map_err(|e| format!("line {line_number}: {e}"))?;
                let record = (signature.to_string(), form_id);
                if !dump.records.contains_key(&record) {
                    return Err(format!(
                        "line {line_number}: field precedes record {signature}:{form_id:08X}"
                    ));
                }
                if dump
                    .fields
                    .insert((record, subrecord.to_string(), ordinal), data)
                    .is_some()
                {
                    return Err(format!("line {line_number}: duplicate field {signature}:{form_id:08X}:{subrecord}[{ordinal}]"));
                }
            }
            _ => return Err(format!("line {line_number}: expected RECORD or FIELD row")),
        }
    }
    Ok(dump)
}

fn validate_signature(value: &str) -> Result<(), String> {
    if value.len() != 4 || !value.is_ascii() {
        return Err(format!("signature must be four ASCII bytes, got {value:?}"));
    }
    Ok(())
}

fn parse_hex_u32(value: &str) -> Result<u32, String> {
    let digits = value.strip_prefix("0x").unwrap_or(value);
    u32::from_str_radix(digits, 16).map_err(|_| format!("invalid hexadecimal FormID {value:?}"))
}

fn parse_hex_bytes(value: &str) -> Result<Vec<u8>, String> {
    if !value.is_ascii() {
        return Err(format!(
            "field bytes must be ASCII hexadecimal, got {value:?}"
        ));
    }
    if !value.len().is_multiple_of(2) {
        return Err(format!("field byte string has odd length {value:?}"));
    }
    (0..value.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&value[i..i + 2], 16)
                .map_err(|_| format!("invalid field bytes {value:?}"))
        })
        .collect()
}

pub(crate) fn from_plugin(plugin: &Plugin) -> Dump {
    let mut dump = Dump::default();
    for record in &plugin.records {
        let signature = record.tag.to_string();
        if !XEDIT_RECORD_SIGNATURES.contains(&signature.as_str()) {
            continue;
        }
        let record_key = (signature, record.form_id);
        *dump.records.entry(record_key.clone()).or_default() += 1;
        let mut ordinals = BTreeMap::<String, usize>::new();
        for field in &record.subrecords {
            let name = field.tag.to_string();
            let ordinal = ordinals.entry(name.clone()).or_default();
            dump.fields
                .insert((record_key.clone(), name, *ordinal), field.data.clone());
            *ordinal += 1;
        }
    }
    dump
}

pub(crate) fn differences(expected: &Dump, actual: &Dump) -> Vec<String> {
    let mut report = Vec::new();
    let expected_counts = counts(&expected.records);
    let actual_counts = counts(&actual.records);
    let signatures: BTreeSet<_> = expected_counts
        .keys()
        .chain(actual_counts.keys())
        .cloned()
        .collect();
    for signature in signatures {
        let left = expected_counts.get(&signature).copied().unwrap_or_default();
        let right = actual_counts.get(&signature).copied().unwrap_or_default();
        if left != right {
            report.push(format!("count {signature}: xEdit={left}, engine={right}"));
        }
    }
    let expected_keys: BTreeSet<_> = expected.records.keys().collect();
    let actual_keys: BTreeSet<_> = actual.records.keys().collect();
    for key in expected_keys.union(&actual_keys) {
        if !expected.records.contains_key(key) {
            report.push(format!("extra engine record {}:{:08X}", key.0, key.1));
        } else if !actual.records.contains_key(key) {
            report.push(format!("missing engine record {}:{:08X}", key.0, key.1));
        }
    }
    for key in expected
        .fields
        .keys()
        .chain(actual.fields.keys())
        .collect::<BTreeSet<_>>()
    {
        match (expected.fields.get(key), actual.fields.get(key)) {
            (Some(left), Some(right)) if left != right => report.push(format!(
                "field {}:{:08X}:{}[{}] differs, xEdit={} engine={}",
                (key.0).0,
                (key.0).1,
                key.1,
                key.2,
                hex(left),
                hex(right),
            )),
            (Some(_), None) => report.push(format!(
                "missing engine field {}:{:08X}:{}[{}]",
                (key.0).0,
                (key.0).1,
                key.1,
                key.2
            )),
            (None, Some(_)) => report.push(format!(
                "extra engine field {}:{:08X}:{}[{}]",
                (key.0).0,
                (key.0).1,
                key.1,
                key.2
            )),
            _ => {}
        }
    }
    report
}

fn counts(records: &BTreeMap<RecordKey, usize>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for ((signature, _), count) in records {
        *counts.entry(signature.clone()).or_default() += count;
    }
    counts
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02X}").expect("writing to a string cannot fail");
    }
    output
}

#[test]
fn tst_09_matches_record_counts_and_field_bytes() {
    let source = "RECORD\tCELL\t00000001\nFIELD\tCELL\t00000001\tDATA\t0\t0100\n";
    let expected = parse_dump(source).unwrap();
    let plugin = Plugin::parse(&fixture_plugin()).unwrap();
    assert!(differences(&expected, &from_plugin(&plugin)).is_empty());
}

#[test]
fn tst_09_compares_only_record_types_selected_for_the_xedit_dump() {
    let expected = parse_dump("RECORD\tCELL\t00000001\n").unwrap();
    let cell = soe_esm::Record::new(soe_esm::Tag::new(b"CELL"), 1);
    let unrelated = soe_esm::Record::new(soe_esm::Tag::new(b"STAT"), 2);
    let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    let cell_index = plugin.push_record(cell);
    let unrelated_index = plugin.push_record(unrelated);
    let mut cell_group = soe_esm::Group::new(soe_esm::GroupKind::Top(soe_esm::Tag::new(b"CELL")));
    cell_group.entries.push(soe_esm::Entry::Record(cell_index));
    let mut unrelated_group =
        soe_esm::Group::new(soe_esm::GroupKind::Top(soe_esm::Tag::new(b"STAT")));
    unrelated_group
        .entries
        .push(soe_esm::Entry::Record(unrelated_index));
    plugin.groups.extend([cell_group, unrelated_group]);

    assert!(differences(&expected, &from_plugin(&plugin)).is_empty());
}

#[test]
fn tst_09_lists_record_count_and_identity_differences() {
    let expected = parse_dump("RECORD\tCELL\t00000001\nRECORD\tREFR\t00000002\n").unwrap();
    let actual = parse_dump("RECORD\tCELL\t00000001\nRECORD\tCELL\t00000003\n").unwrap();
    let differences = differences(&expected, &actual);
    assert!(differences
        .iter()
        .any(|line| line == "count CELL: xEdit=1, engine=2"));
    assert!(differences
        .iter()
        .any(|line| line == "count REFR: xEdit=1, engine=0"));
    assert!(differences
        .iter()
        .any(|line| line == "missing engine record REFR:00000002"));
    assert!(differences
        .iter()
        .any(|line| line == "extra engine record CELL:00000003"));
}

#[test]
fn tst_09_lists_changed_missing_and_extra_fields() {
    let expected = parse_dump(concat!(
        "RECORD\tCELL\t00000001\n",
        "FIELD\tCELL\t00000001\tDATA\t0\t0100\n",
        "FIELD\tCELL\t00000001\tFULL\t0\t4869\n",
    ))
    .unwrap();
    let actual = parse_dump(concat!(
        "RECORD\tCELL\t00000001\n",
        "FIELD\tCELL\t00000001\tDATA\t0\t0200\n",
        "FIELD\tCELL\t00000001\tEDID\t0\t58\n",
    ))
    .unwrap();
    let differences = differences(&expected, &actual);
    assert!(differences
        .iter()
        .any(|line| line.contains("DATA[0] differs")));
    assert!(differences
        .iter()
        .any(|line| line.contains("missing engine field CELL:00000001:FULL[0]")));
    assert!(differences
        .iter()
        .any(|line| line.contains("extra engine field CELL:00000001:EDID[0]")));
}

#[test]
fn tst_09_rejects_malformed_dump_rows() {
    assert!(parse_dump("FIELD\tCELL\t00000001\tDATA\t0\t00\n")
        .unwrap_err()
        .contains("precedes record"));
    assert!(
        parse_dump("RECORD\tCELL\t00000001\nFIELD\tCELL\t00000001\tDATA\t0\t0\n")
            .unwrap_err()
            .contains("odd length")
    );
    assert!(parse_dump("RECORD\tBAD\t00000001\n")
        .unwrap_err()
        .contains("four ASCII bytes"));
}

#[test]
fn tst_09_compares_the_installed_skyrim_master_when_dump_is_configured() {
    let (Some(data), Some(dump_path)) = (
        std::env::var_os("SKYRIM_DATA").map(PathBuf::from),
        std::env::var_os("SOE_XEDIT_DUMP").map(PathBuf::from),
    ) else {
        eprintln!("SKYRIM_DATA and SOE_XEDIT_DUMP are required, skipping xEdit differential");
        return;
    };
    let expected_text = std::fs::read_to_string(&dump_path)
        .unwrap_or_else(|error| panic!("{}: {error}", dump_path.display()));
    let expected = parse_dump(&expected_text)
        .unwrap_or_else(|error| panic!("{}: {error}", dump_path.display()));
    let path = data.join("Skyrim.esm");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let plugin =
        Plugin::parse(&bytes).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let found = differences(&expected, &from_plugin(&plugin));
    eprintln!(
        "xEdit differential: {} xEdit records, {} engine records, {} differences",
        expected.records.len(),
        plugin.records.len(),
        found.len()
    );
    assert!(
        found.is_empty(),
        "xEdit differential differences:\n{}",
        found.join("\n")
    );
}

fn fixture_plugin() -> Vec<u8> {
    let record = soe_esm::Record::new(Tag::new(b"CELL"), 1).sub(b"DATA", [1u8, 0]);
    let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader::default());
    let index = plugin.push_record(record);
    let mut top = soe_esm::Group::new(soe_esm::GroupKind::Top(Tag::new(b"CELL")));
    top.entries.push(soe_esm::Entry::Record(index));
    plugin.groups.push(top);
    plugin.to_bytes()
}
