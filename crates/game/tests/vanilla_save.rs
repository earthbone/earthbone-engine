// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Saved identities must survive load-order changes before world state can
//! be applied. Fixtures exercise distinct owners with the same local ID.

#[path = "vanilla_save/reference_extras.rs"]
mod reference_extras;

#[path = "vanilla_save/npcs.rs"]
mod npcs;

#[path = "vanilla_save/references.rs"]
mod references;

#[path = "vanilla_save/inventory.rs"]
mod inventory;

#[path = "vanilla_save/actor_sources.rs"]
mod actor_sources;

#[path = "vanilla_save/lists.rs"]
mod lists;

#[path = "vanilla_save/appa_idlm.rs"]
mod appa_idlm;
#[path = "vanilla_save/base_objects.rs"]
mod base_objects;
#[path = "vanilla_save/static_objects.rs"]
mod static_objects;
#[path = "vanilla_save/woop_mgef.rs"]
mod woop_mgef;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use soe_esm::{
    plugin::{flags, FileHeader},
    GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{
    vanilla_save::{RestoreError, SavedFormMap},
    world::DYNAMIC_FILE,
    Game, World,
};
use soe_papyrus::{
    asm::{ident, s, FunctionBuilder, ScriptBuilder},
    Opcode, Value,
};
use soe_save::{
    Compression, GlobalData, GlobalVariable, GlobalVariables, Header, RefId, Save, WString,
};

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn plugin(light: bool) -> Plugin {
    let mut plugin = Plugin::new(FileHeader {
        flags: if light { flags::LIGHT_MASTER } else { 0 },
        ..Default::default()
    });
    plugin.push_top(
        Record::new(Tag(*b"GLOB"), 0x800).sub(Tag(*b"FLTV"), 11.0_f32.to_le_bytes().to_vec()),
    );
    plugin
}

fn order() -> LoadOrder {
    let mut order = LoadOrder::new();
    for (name, light) in [
        ("Skyrim.esm", false),
        ("Extra.esl", true),
        ("B.esp", false),
        ("A.esp", false),
        ("Light.esl", true),
    ] {
        order.add(name, plugin(light)).unwrap();
    }
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into(), "A.esp".into()],
        ..Default::default()
    });
    patch.push_top(
        Record::new(Tag(*b"GLOB"), 0x0100_0800).sub(Tag(*b"FLTV"), 99.0_f32.to_le_bytes().to_vec()),
    );
    order.add("Patch.esp", patch).unwrap();
    order
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Prisoner"),
        player_level: 1,
        player_location: WString::default(),
        game_date: WString::default(),
        player_race: WString::default(),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 0.0,
        filetime: 0,
        screenshot_width: 0,
        screenshot_height: 0,
        compression: Compression::None,
        screenshot: vec![],
    };
    let mut bytes = vec![];
    header.write(&mut bytes);
    let mut plugins = vec![1];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let at = bytes.len() as u32 + 100;
    let table = soe_save::FileLocationTable {
        form_id_array_count_offset: at + 8,
        unknown_table_3_offset: at + 16,
        table_1_offset: at,
        table_2_offset: at,
        change_forms_offset: at,
        table_3_offset: at,
        table_1_count: 0,
        table_2_count: 0,
        table_3_count: 0,
        change_form_count: 0,
        unused: [0; 15],
    };
    table.write(&mut bytes);
    GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = ["skyrim.ESM", "a.ESP", "B.esp"]
        .map(WString::from_text)
        .to_vec();
    save.light_plugins = vec![WString::from_text("LIGHT.esl")];
    save
}

fn globals(save: &mut Save, values: &[(u32, f32)]) {
    save.form_id_array = values.iter().map(|(raw, _)| *raw).collect();
    let globals = GlobalVariables {
        variables: values
            .iter()
            .enumerate()
            .map(|(n, (_, value))| GlobalVariable {
                form: reference(n as u32 + 1),
                value: *value,
            })
            .collect(),
    };
    save.table_1 = vec![GlobalData {
        kind: 3,
        data: globals.write().unwrap(),
    }];
}

#[test]
fn saved_slots_resolve_by_owner_name_after_regular_and_light_reordering() {
    let order = order();
    let mut save = save();
    save.form_id_array = vec![0x0100_0800, 0xFE00_0800, 0xFFAB_CDEF, 0];
    let map = SavedFormMap::new(&save, &order).unwrap();
    for (raw, expected) in [
        (1, Some(id(3, 0x800))),
        (2, Some(id(4, 0x800))),
        (3, Some(id(DYNAMIC_FILE, 0xAB_CDEF))),
        (4, None),
        (0x400800, Some(id(0, 0x800))),
        (0x800123, Some(id(DYNAMIC_FILE, 0x123))),
        (0, None),
    ] {
        assert_eq!(map.reference(reference(raw)).unwrap(), expected);
    }
    assert_eq!(
        order.runtime_id(map.form_id(0xFE00_0800).unwrap().unwrap()),
        0xFE00_1800
    );
}

#[test]
fn unresolved_references_and_slots_do_not_alias_valid_forms() {
    let order = order();
    let save = save();
    let map = SavedFormMap::new(&save, &order).unwrap();
    for raw in [1, 0xC00001] {
        assert_eq!(
            map.reference(reference(raw)),
            Err(RestoreError::Reference(raw))
        );
    }
    for raw in [0x0300_0800, 0xFE00_1800] {
        assert_eq!(map.form_id(raw), Err(RestoreError::PluginSlot(raw)));
    }
}

#[test]
fn changed_or_missing_plugin_identity_is_rejected_before_world_mutation() {
    let game = Game::new(order());
    let before = game.save_state().unwrap();
    for name in ["Missing.esp", "Extra.esl", "A.ESP"] {
        let mut save = save();
        save.plugins.push(WString::from_text(name));
        assert!(game.world_mut().restore_vanilla_globals(&save).is_err());
        assert_eq!(game.save_state().unwrap(), before);
    }
    let mut save = save();
    save.light_plugins.push(WString::from_text("A.esp"));
    assert!(matches!(
        SavedFormMap::new(&save, game.world().order()),
        Err(RestoreError::PluginLists(_))
    ));
}

#[test]
fn ambiguous_current_names_are_rejected() {
    let save = save();
    let mut order = order();
    order.add("a.ESP", plugin(false)).unwrap();
    assert!(matches!(
        SavedFormMap::new(&save, &order),
        Err(RestoreError::PluginLists(_))
    ));
}

#[test]
fn sav_07_full_saved_regular_and_light_lists_keep_created_identities_separate() {
    for regular_at_255 in [false, true] {
        let mut save = save();
        save.plugins = (0..254)
            .map(|n| WString::from_text(&format!("Regular{n}.esp")))
            .collect();
        save.light_plugins = (0..4096)
            .map(|n| WString::from_text(&format!("Light{n}.esl")))
            .collect();
        let mut order = LoadOrder::new();
        let prefix = if regular_at_255 { 2 } else { 0 };
        let names = save
            .light_plugins
            .iter()
            .rev()
            .take(prefix)
            .map(|n| (n, true))
            .chain(save.plugins.iter().rev().map(|n| (n, false)))
            .chain(
                save.light_plugins
                    .iter()
                    .rev()
                    .skip(prefix)
                    .map(|n| (n, true)),
            );
        for (name, light) in names {
            order.add(&name.to_text(), plugin(light)).unwrap();
        }
        let raw_ids: Vec<_> = (0..254)
            .map(|slot| (slot << 24) | 0x800)
            .chain((0..4096).map(|slot| 0xFE00_0800 | (slot << 12)))
            .chain([0xFF00_0800, 0xFFFF_FFFF])
            .collect();
        save.form_id_array.clone_from(&raw_ids);
        let map = SavedFormMap::new(&save, &order).unwrap();
        let mut identities = std::collections::BTreeSet::new();
        for (at, raw) in raw_ids.iter().copied().enumerate() {
            let mapped = map.form_id(raw).unwrap().unwrap();
            assert!(
                identities.insert(mapped),
                "aliased saved FormID {raw:#010x}"
            );
            assert_eq!(
                map.reference(reference(at as u32 + 1)).unwrap(),
                Some(mapped)
            );
            let expected_file = match raw >> 24 {
                0xFF => DYNAMIC_FILE,
                0xFE => order
                    .position_of(&save.light_plugins[((raw >> 12) & 0xFFF) as usize].to_text())
                    .unwrap() as u16,
                slot => order
                    .position_of(&save.plugins[slot as usize].to_text())
                    .unwrap() as u16,
            };
            assert_eq!(mapped.file, expected_file);
            if mapped.file == DYNAMIC_FILE {
                assert_eq!(order.runtime_id(mapped), raw);
                assert!(!order.contains(mapped));
            } else {
                assert!(order.contains(mapped));
            }
        }
        assert!(identities.contains(&id(255, 0x800)));
        assert_eq!(
            map.reference(reference(0x800800)).unwrap(),
            Some(id(DYNAMIC_FILE, 0x800))
        );
        assert_eq!(map.form_id(0).unwrap(), None);
    }
}

#[test]
fn restored_globals_reach_live_objects_and_engine_saves_without_events() {
    let mut game = Game::new(order());
    let mut save = save();
    globals(&mut save, &[(0x0100_0800, 42.5), (0xFE00_0800, -3.25)]);
    let object = game.object_of_form(id(3, 0x800));
    assert_eq!(game.world_mut().global_value(id(3, 0x800)), 99.0);
    assert_eq!(game.world_mut().restore_vanilla_globals(&save).unwrap(), 2);
    let world = game.world();
    assert_eq!(world.form_of(object), Some(id(3, 0x800)));
    assert!(world.pending.is_empty());
    drop(world);
    assert_eq!(game.world_mut().global_value(id(3, 0x800)), 42.5);
    assert_eq!(game.world_mut().global_value(id(4, 0x800)), -3.25);
    assert_eq!(game.world_mut().global_value(id(2, 0x800)), 11.0);
    let saved = game.save_state().unwrap();
    let mut restored = Game::new(order());
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world_mut().global_value(id(3, 0x800)), 42.5);
    assert_eq!(restored.world_mut().global_value(id(4, 0x800)), -3.25);
    let script = ScriptBuilder::new("ReadRestoredGlobal", "Form")
        .function(
            FunctionBuilder::new("Read", "Float")
                .global()
                .param("target", "GlobalVariable")
                .local("result", "Float")
                .ins(
                    Opcode::CallMethod,
                    vec![s("GetValue"), ident("target"), ident("result")],
                )
                .ins(Opcode::Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    restored.load_script(&script).unwrap();
    let object = restored.object_of_form(id(3, 0x800));
    let thread = restored
        .vm
        .call_global("ReadRestoredGlobal", "Read", &[Value::Object(object)])
        .unwrap();
    restored.run_until_idle(64);
    assert_eq!(restored.vm.thread_result(thread), Some(Value::Float(42.5)));
}

#[test]
fn engine_defined_playcredits_global_has_a_record_and_accepts_saved_state() {
    let mut world = World::new(order());
    assert_eq!(world.form("PlayCredits"), Some(id(0, 0x63)));
    let mut save = save();
    globals(&mut save, &[(0x63, 1.0)]);
    world.restore_vanilla_globals(&save).unwrap();
    assert_eq!(world.global_value(id(0, 0x63)), 1.0);
}

#[test]
fn late_invalid_or_repeated_globals_leave_the_entire_game_unchanged() {
    let game = Game::new(order());
    let before = game.save_state().unwrap();
    for invalid in [0, 0x0100_0999, 0xFF00_0800, 0x0100_0800] {
        let mut save = save();
        globals(&mut save, &[(0x0100_0800, 42.5), (invalid, 99.0)]);
        assert!(game.world_mut().restore_vanilla_globals(&save).is_err());
        assert_eq!(game.save_state().unwrap(), before);
    }
}

#[test]
fn deleted_and_wrong_type_winners_are_not_restored_as_globals() {
    for tag in [Tag(*b"BOOK"), Tag(*b"GLOB")] {
        let mut order = order();
        let mut patch = Plugin::new(FileHeader {
            masters: vec!["Skyrim.esm".into(), "A.esp".into()],
            ..Default::default()
        });
        let mut record = Record::new(tag, 0x0100_0800);
        if tag == Tag(*b"GLOB") {
            record.flags |= flags::DELETED;
        }
        patch.push_top(record);
        order.add("Invalid.esp", patch).unwrap();
        let mut world = World::new(order);
        let mut save = save();
        globals(&mut save, &[(0x0100_0800, 4.0)]);
        let before = world.save_state();
        assert!(matches!(
            world.restore_vanilla_globals(&save),
            Err(RestoreError::GlobalTarget(_))
        ));
        assert_eq!(world.save_state(), before);
    }
}

#[test]
fn malformed_absent_and_duplicate_global_tables_do_not_apply() {
    let mut world = World::new(order());
    let mut save = save();
    let before = world.save_state();
    assert_eq!(
        world.restore_vanilla_globals(&save),
        Err(RestoreError::GlobalBlocks(0))
    );
    globals(&mut save, &[(0x0100_0800, 42.5)]);
    let block = save.table_1[0].clone();
    for end in 0..block.data.len() {
        save.table_1[0].data = block.data[..end].to_vec();
        assert!(world.restore_vanilla_globals(&save).is_err());
        assert_eq!(world.save_state(), before);
    }
    save.table_1[0] = block.clone();
    save.table_1[0].data.push(0);
    assert!(world.restore_vanilla_globals(&save).is_err());
    save.table_1 = vec![block.clone(), block];
    assert_eq!(
        world.restore_vanilla_globals(&save),
        Err(RestoreError::GlobalBlocks(2))
    );
    assert_eq!(world.save_state(), before);
}

#[test]
fn global_payloads_keep_float_bits_and_allow_wide_counts() {
    for bits in [0x8000_0000, 0x7FC0_1234, 0x7F80_0000] {
        let mut save = save();
        globals(&mut save, &[(0x0100_0800, f32::from_bits(bits))]);
        save.table_1[0].data.splice(..1, [5, 0]);
        let mut world = World::new(order());
        world.restore_vanilla_globals(&save).unwrap();
        assert_eq!(world.global_value(id(3, 0x800)).to_bits(), bits);
    }
    assert!(GlobalVariables::read(&[0xFE, 0xFF, 0xFF]).is_err());
}

fn collect_saves(path: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_saves(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            out.push(path);
        }
    }
}

#[test]
fn installed_save_globals_restore_into_live_forms_and_persist() {
    let (Some(data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES not set, skipping global restoration survey");
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
    let order = Arc::new(order);
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    // CONTEXT: The supplied corpus has sixty saves and more than 900 globals each.
    assert!(paths.len() >= 50, "expected the full save survey");
    let mut total = 0;
    for path in &paths {
        let save = Save::open(path).unwrap();
        let map = SavedFormMap::new(&save, &order).unwrap();
        let expected: Vec<_> = save
            .globals()
            .unwrap()
            .variables
            .iter()
            .map(|global| {
                (
                    map.reference(global.form).unwrap().unwrap(),
                    global.value.to_bits(),
                )
            })
            .collect();
        assert!(expected.len() > 500, "{}: too few globals", path.display());
        let mut world = World::from_shared_order(Arc::clone(&order));
        let restored_count = world
            .restore_vanilla_globals(&save)
            .unwrap_or_else(|error| {
                let invalid: Vec<_> = expected
                    .iter()
                    .filter_map(|(id, _)| {
                        let record = order.winner(*id);
                        (!record.is_some_and(|r| r.tag == Tag(*b"GLOB") && !r.is_deleted()))
                            .then(|| (*id, record.map(|r| (r.tag, r.editor_id(), r.flags))))
                    })
                    .collect();
                panic!("{}: {error}, invalid globals: {invalid:?}", path.display());
            });
        assert_eq!(restored_count, expected.len());
        for &(id, bits) in &expected {
            assert_eq!(world.global_value(id).to_bits(), bits);
        }
        assert!(world.pending.is_empty());
        let hash = world.state_hash();
        let mut restored = World::from_shared_order(Arc::clone(&order));
        restored.load_state(&world.save_state()).unwrap();
        assert_eq!(restored.state_hash(), hash);
        for &(id, bits) in &expected {
            assert_eq!(restored.global_value(id).to_bits(), bits);
        }
        total += expected.len();
    }
    eprintln!(
        "{} saves, {total} globals restored into live forms and preserved through engine saves",
        paths.len()
    );
}

#[path = "vanilla_save/quests.rs"]
mod quests;

#[path = "vanilla_save/calendar.rs"]
mod calendar;
