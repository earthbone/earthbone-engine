// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Full quest payloads must retain field order, optional sections and count
//! encodings. The external corpus stays outside the repository.

use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Tag};
use soe_save::{
    quest::{flags, EventParameter, QuestChange, QuestStaticData},
    ChangeForm, RefId, Save,
};

fn u32_bytes(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn reference(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&[(value >> 16) as u8, (value >> 8) as u8, value as u8]);
}

fn full_payload() -> Vec<u8> {
    let mut out = vec![];
    u32_bytes(&mut out, 0x1234_5678);
    out.extend_from_slice(&0xABCD_u16.to_le_bytes());
    out.extend_from_slice(&0x0843_u16.to_le_bytes());
    u32_bytes(&mut out, 0x7FC0_0123);
    out.extend_from_slice(&[5, 0]);
    out.extend_from_slice(&i16::MIN.to_le_bytes());
    out.push(7);
    out.push(4);
    u32_bytes(&mut out, 123);
    u32_bytes(&mut out, 5);
    out.push(42);
    u32_bytes(&mut out, 2);
    u32_bytes(&mut out, 10);
    out.push(0);
    reference(&mut out, 0x400014);
    u32_bytes(&mut out, 11);
    out.push(1);
    for raw in [0x800123, 0x400456, 0x400789, 0, 0x401ABC] {
        reference(&mut out, raw);
    }
    u32_bytes(&mut out, 1);
    u32_bytes(&mut out, 12);
    reference(&mut out, 0x400789);
    out.push(1);
    u32_bytes(&mut out, 987);
    out.extend_from_slice(b"ADDI");
    u32_bytes(&mut out, 5);
    for kind in 0..=4 {
        u32_bytes(&mut out, kind);
        if kind == 3 {
            u32_bytes(&mut out, 0xDEAD_BEEF);
        } else {
            reference(&mut out, 0x400014 + kind);
        }
    }
    u32_bytes(&mut out, 456);
    out.extend_from_slice(&[6, 0, 0, 0]);
    u32_bytes(&mut out, 789);
    out.push(4);
    u32_bytes(&mut out, 10);
    reference(&mut out, 0x400014);
    out.push(4);
    reference(&mut out, 0x400039);
    u32_bytes(&mut out, 0x8000_0000);
    out.extend_from_slice(&234_u16.to_le_bytes());
    out.push(0xFE);
    out.push(0x80);
    out
}

fn change(flags: u32, data: Vec<u8>) -> ChangeForm {
    ChangeForm {
        form: RefId::from_bytes([0x40, 0x08, 0]),
        change_flags: flags,
        type_field: 8,
        version: 74,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn quest_sections_keep_distinct_runtime_and_instance_state() {
    let payload = full_payload();
    let decoded = change(flags::KNOWN, payload.clone())
        .quest_change()
        .unwrap()
        .unwrap();
    assert_eq!(decoded.form_flags.unwrap().flags, 0x1234_5678);
    assert_eq!(decoded.form_flags.unwrap().unknown, 0xABCD);
    assert_eq!(decoded.quest_flags, Some(0x0843));
    assert_eq!(decoded.script_delay.unwrap().to_bits(), 0x7FC0_0123);
    let stage = decoded.stages.as_ref().unwrap().entries[0];
    assert_eq!(
        (stage.stage, stage.status, stage.is_done()),
        (i16::MIN, 7, true)
    );
    let objective = decoded.objectives.as_ref().unwrap().entries[0];
    assert_eq!((objective.id, objective.status), (123, 5));
    let runtime = decoded.runtime.as_ref().unwrap();
    assert_eq!(runtime.unknown, 42);
    assert_eq!(runtime.aliases[0].references[0].raw(), 0x400014);
    assert_eq!(
        runtime.aliases[1]
            .references
            .iter()
            .map(|r| r.raw())
            .collect::<Vec<_>>(),
        [0x800123, 0x400456, 0x400789, 0, 0x401ABC]
    );
    assert_eq!(runtime.locations[0].id, 12);
    let event = runtime.event.as_ref().unwrap();
    assert_eq!((event.id, event.code), (987, *b"ADDI"));
    assert_eq!(event.parameters[3], EventParameter::Integer(0xDEAD_BEEF));
    assert!(matches!(
        event.parameters[0],
        EventParameter::Reference { kind: 0, .. }
    ));
    let instances = decoded.instances.as_ref().unwrap();
    assert_eq!(instances.unknown, 456);
    let instance = &instances.instances.entries[0];
    assert_eq!(instance.unknown, 789);
    assert_eq!(instance.aliases.entries[0].reference.raw(), 0x400014);
    assert_eq!(instance.globals.entries[0].value.to_bits(), 0x8000_0000);
    assert_eq!((instance.unknown_short, instance.unknown_byte), (234, 0xFE));
    assert_eq!(decoded.already_run, Some(0x80));
    assert_eq!(decoded.data_bytes().unwrap(), payload);
}

#[test]
fn every_quest_presence_combination_consumes_only_its_sections() {
    let bits = [
        flags::FORM,
        flags::QUEST,
        flags::SCRIPT_DELAY,
        flags::STAGES,
        flags::OBJECTIVES,
        flags::RUNTIME,
        flags::INSTANCES,
        flags::ALREADY_RUN,
        flags::SCRIPT,
    ];
    for mask in 0..512 {
        let mut flags = 0;
        let mut payload = vec![];
        for (index, &bit) in bits.iter().enumerate() {
            if mask & (1 << index) == 0 {
                continue;
            }
            flags |= bit;
            match bit {
                flags::FORM => payload.extend_from_slice(&[0; 6]),
                flags::QUEST => payload.extend_from_slice(&[0; 2]),
                flags::SCRIPT_DELAY => payload.extend_from_slice(&[0; 4]),
                flags::STAGES | flags::OBJECTIVES | flags::ALREADY_RUN => payload.push(0),
                flags::RUNTIME => payload.extend_from_slice(&[0; 10]),
                flags::INSTANCES => payload.extend_from_slice(&[0; 5]),
                flags::SCRIPT => {}
                _ => unreachable!(),
            }
        }
        let decoded = QuestChange::read(flags, &payload).unwrap();
        assert_eq!(
            decoded.data_bytes().unwrap(),
            payload,
            "flags {flags:#010x}"
        );
        assert_eq!(decoded.stages.is_some(), flags & flags::STAGES != 0);
        assert_eq!(decoded.instances.is_some(), flags & flags::INSTANCES != 0);
    }
}

#[test]
fn truncated_quest_payloads_and_trailing_data_are_rejected() {
    let mut payload = full_payload();
    for end in 0..payload.len() {
        assert!(
            QuestChange::read(flags::KNOWN, &payload[..end]).is_err(),
            "prefix {end}"
        );
    }
    payload.push(0);
    assert!(QuestChange::read(flags::KNOWN, &payload).is_err());
    assert!(QuestChange::read(1 << 12, &[]).is_err());
}

#[test]
fn impossible_counts_fail_before_allocation() {
    for flag in [flags::STAGES, flags::OBJECTIVES] {
        assert!(QuestChange::read(flag, &[0xFE, 0xFF, 0xFF]).is_err());
    }
    assert!(QuestChange::read(flags::RUNTIME, &[0, 0xFF, 0xFF, 0xFF, 0xFF]).is_err());
    assert!(QuestChange::read(flags::INSTANCES, &[0, 0, 0, 0, 0xFE, 0xFF, 0xFF]).is_err());
}

#[test]
fn unknown_story_event_types_fail_instead_of_desynchronizing_the_payload() {
    let mut runtime = vec![0; 10];
    runtime[9] = 1;
    runtime.extend_from_slice(&[0; 8]);
    u32_bytes(&mut runtime, 1);
    u32_bytes(&mut runtime, 5);
    runtime.extend_from_slice(&[0; 4]);
    let error = QuestChange::read(flags::RUNTIME, &runtime).unwrap_err();
    assert!(error.to_string().contains("parameter type 5"));
}

#[test]
fn diagnostic_encoding_rejects_inconsistent_mutations() {
    let decoded = QuestChange::read(flags::KNOWN, &full_payload()).unwrap();
    let mut changed = decoded.clone();
    changed.stages.as_mut().unwrap().entries.clear();
    assert!(changed.data_bytes().is_err());
    let mut changed = decoded.clone();
    changed.runtime.as_mut().unwrap().aliases[0]
        .references
        .clear();
    assert!(changed.data_bytes().is_err());
    let mut changed = decoded.clone();
    changed.runtime.as_mut().unwrap().event_flag = 0;
    assert!(changed.data_bytes().is_err());
    let mut changed = decoded.clone();
    changed.quest_flags = None;
    assert!(changed.data_bytes().is_err());
    let mut changed = decoded;
    changed
        .runtime
        .as_mut()
        .unwrap()
        .event
        .as_mut()
        .unwrap()
        .parameters[0] = EventParameter::Reference {
        kind: 5,
        reference: RefId::from_bytes([0; 3]),
    };
    assert!(changed.data_bytes().is_err());
}

#[test]
fn compressed_quest_payloads_use_the_same_complete_decoder() {
    let payload = full_payload();
    let mut encoder = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::default());
    encoder.write_all(&payload).unwrap();
    let mut change = change(flags::KNOWN, encoder.finish().unwrap());
    change.uncompressed_len = payload.len() as u32;
    let decoded = change.quest_change().unwrap().unwrap();
    assert_eq!(decoded.data_bytes().unwrap(), payload);
    change.uncompressed_len += 1;
    assert!(change.quest_change().is_err());
    change.type_field = 9;
    assert!(change.quest_change().unwrap().is_none());
}

#[test]
fn quest_static_data_preserves_array_roles_and_wide_counts() {
    let mut bytes = vec![];
    for n in 0..2 {
        u32_bytes(&mut bytes, 1);
        u32_bytes(&mut bytes, n + 100);
        bytes.extend_from_slice(b"ADDI");
        u32_bytes(&mut bytes, 1);
        u32_bytes(&mut bytes, 0);
        reference(&mut bytes, 0x400014 + n);
    }
    for n in 0..3 {
        u32_bytes(&mut bytes, 1);
        reference(&mut bytes, 0x400800 + n);
    }
    bytes.extend_from_slice(&[5, 0]);
    reference(&mut bytes, 0x400900);
    bytes.extend_from_slice(&[6, 0, 0, 0]);
    u32_bytes(&mut bytes, 0x1234_5678);
    u32_bytes(&mut bytes, 0xABCD_EF01);
    bytes.push(1);
    let decoded = QuestStaticData::read(&bytes).unwrap();
    assert_eq!(decoded.events[1][0].id, 101);
    assert_eq!(decoded.references[2][0].raw(), 0x400802);
    assert_eq!(
        decoded.entries.entries[0].values.entries[0],
        [0x1234_5678, 0xABCD_EF01]
    );
    assert_eq!(decoded.data_bytes().unwrap(), bytes);
    for end in 0..bytes.len() {
        assert!(
            QuestStaticData::read(&bytes[..end]).is_err(),
            "prefix {end}"
        );
    }
    bytes.push(0);
    assert!(QuestStaticData::read(&bytes).is_err());
    assert!(QuestStaticData::read(&[0xFF; 4]).is_err());
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            files.push(path);
        }
    }
}

#[test]
fn saved_objectives_and_alias_ids_belong_to_installed_quest_definitions() {
    let (Some(data), Some(root)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping quest identity survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(Path::new(&data).join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut files = vec![];
    collect(Path::new(&root), &mut files);
    assert!(files.len() >= 50, "full corpus required");
    files.sort();
    let mut checks = [0_usize; 4];
    let mut statuses = BTreeMap::<u32, usize>::new();
    for file in &files {
        let save = Save::open(file).unwrap();
        assert!(
            save.light_plugins.is_empty(),
            "this survey expects the supplied five-master saves"
        );
        for change in save
            .change_forms
            .iter()
            .filter(|change| change.record_type() == "QUST")
        {
            let raw = save.resolve(change.form).unwrap();
            let plugin = &save.plugins[(raw >> 24) as usize];
            let id = GlobalFormId {
                file: order.position_of(&plugin.to_text()).unwrap() as u16,
                object: raw & 0x00FF_FFFF,
            };
            let record = order.winner(id).unwrap();
            assert_eq!(record.tag, Tag(*b"QUST"), "{id}");
            let objectives: Vec<_> = record
                .all(Tag(*b"QOBJ"))
                .map(|bytes| {
                    assert_eq!(bytes.len(), 2);
                    u32::from(u16::from_le_bytes(bytes.try_into().unwrap()))
                })
                .collect();
            let alias_ids = |tag| {
                record
                    .all(tag)
                    .map(|bytes| {
                        assert_eq!(bytes.len(), 4);
                        u32::from_le_bytes(bytes.try_into().unwrap())
                    })
                    .collect::<Vec<_>>()
            };
            let reference_aliases = alias_ids(Tag(*b"ALST"));
            let location_aliases = alias_ids(Tag(*b"ALLS"));
            let decoded = change.quest_change().unwrap().unwrap();
            if let Some(objectives_data) = decoded.objectives {
                for objective in objectives_data.entries {
                    assert!(
                        objectives.contains(&objective.id),
                        "{} {id}: unknown objective {}",
                        file.display(),
                        objective.id
                    );
                    assert!(
                        objective.status <= 5,
                        "unexpected objective status {}",
                        objective.status
                    );
                    *statuses.entry(objective.status).or_default() += 1;
                    checks[0] += 1;
                }
            }
            if let Some(runtime) = decoded.runtime {
                for alias in runtime.aliases {
                    assert!(
                        reference_aliases.contains(&alias.id),
                        "{} {id}: unknown reference alias {}",
                        file.display(),
                        alias.id
                    );
                    checks[1] += 1;
                }
                for location in runtime.locations {
                    assert!(
                        location_aliases.contains(&location.id),
                        "{} {id}: unknown location alias {}",
                        file.display(),
                        location.id
                    );
                    checks[2] += 1;
                }
            }
            if let Some(instances) = decoded.instances {
                for instance in instances.instances.entries {
                    for alias in instance.aliases.entries {
                        assert!(
                            reference_aliases.contains(&alias.id)
                                || location_aliases.contains(&alias.id),
                            "{} {id}: unknown instance alias {}",
                            file.display(),
                            alias.id
                        );
                        checks[3] += 1;
                    }
                }
            }
        }
    }
    assert!(
        checks.iter().all(|count| *count > 100),
        "insufficient survey coverage: {checks:?}"
    );
    eprintln!("installed quest identities: objectives={}, reference aliases={}, location aliases={}, instance aliases={}, statuses={statuses:?}", checks[0], checks[1], checks[2], checks[3]);
}

#[test]
fn every_external_quest_payload_round_trips_through_typed_state() {
    let Some(root) = std::env::var_os("SOE_TEST_SAVES") else {
        eprintln!("SOE_TEST_SAVES unset, skipping quest state corpus");
        return;
    };
    let mut files = vec![];
    collect(Path::new(&root), &mut files);
    files.sort();
    // CONTEXT: The supplied survey contains sixty saves, each with many quests.
    assert!(files.len() >= 50, "full corpus required");
    let mut counts = [0_usize; 8];
    let mut header_flags = BTreeMap::<u32, usize>::new();
    let mut event_tags = BTreeMap::<u32, usize>::new();
    let mut static_bytes = 0;
    let mut static_events = 0;
    for file in &files {
        let save = Save::open(file).unwrap();
        let static_data = save
            .quest_static_data()
            .unwrap()
            .expect("quest static block");
        let block = save.table_2.iter().find(|block| block.kind == 107).unwrap();
        assert_eq!(
            static_data.data_bytes().unwrap(),
            block.data,
            "{}: static data",
            file.display()
        );
        static_bytes += block.data.len();
        static_events += static_data.events.iter().map(Vec::len).sum::<usize>();
        let mut in_save = 0;
        for change in save
            .change_forms
            .iter()
            .filter(|form| form.record_type() == "QUST")
        {
            let decoded = change
                .quest_change()
                .unwrap_or_else(|error| {
                    panic!(
                        "{} {:?} flags={:#010x}: {error}",
                        file.display(),
                        change.form,
                        change.change_flags
                    )
                })
                .unwrap();
            assert_eq!(
                decoded.data_bytes().unwrap(),
                change.payload().unwrap().as_ref(),
                "{} {:?}",
                file.display(),
                change.form
            );
            *header_flags.entry(change.change_flags).or_default() += 1;
            counts[0] += 1;
            counts[1] += change.payload().unwrap().len();
            if let Some(stages) = &decoded.stages {
                counts[2] += stages.entries.len();
                assert_eq!(change.quest_stages().unwrap().unwrap(), stages.entries);
            }
            if let Some(objectives) = &decoded.objectives {
                counts[3] += objectives.entries.len();
            }
            if let Some(runtime) = &decoded.runtime {
                counts[4] += runtime.aliases.len() + runtime.locations.len();
                if let Some(event) = &runtime.event {
                    counts[5] += 1;
                    for parameter in &event.parameters {
                        let kind = match parameter {
                            EventParameter::Reference { kind, .. } => *kind,
                            EventParameter::Integer(_) => 3,
                        };
                        *event_tags.entry(kind).or_default() += 1;
                    }
                }
            }
            if let Some(instances) = &decoded.instances {
                counts[6] += instances.instances.entries.len();
                counts[7] += instances
                    .instances
                    .entries
                    .iter()
                    .map(|instance| instance.globals.entries.len())
                    .sum::<usize>();
            }
            in_save += 1;
        }
        assert!(in_save > 100, "{}: too few quest records", file.display());
    }
    assert!(counts[2] > 1000 && counts[3] > 100 && counts[4] > 100 && counts[6] > 100);
    eprintln!("{} saves: quests={}, bytes={}, stages={}, objectives={}, aliases={}, events={}, instances={}, instance globals={}", files.len(), counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], counts[6], counts[7]);
    eprintln!("header flags={header_flags:?}, event parameter tags={event_tags:?}");
    eprintln!("quest static data: {static_bytes} bytes, {static_events} events");
}
