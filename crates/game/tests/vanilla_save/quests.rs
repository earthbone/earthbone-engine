// SPDX-License-Identifier: GPL-3.0-or-later

//! Quest restoration exercises live natives and persistence against independently
//! authored QUST bytes, then compares every quest in the external save corpus.

use super::*;
use soe_game::quest;
use soe_save::{quest::flags as change_flags, ChangeForm};

const QUEST: u32 = 0x900;

fn fixture() -> Game {
    let mut order = order();
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into(), "A.esp".into()],
        ..Default::default()
    });
    let mut record = quest::build_record(0x0100_0900, "RestoredQuest", 0x0100);
    for stage in [0, 10, 20] {
        record = quest::with_stage(record, stage, 0, 0, "");
    }
    for objective in 0..6 {
        record = quest::with_objective(record, objective, "");
    }
    record = quest::with_alias(record, 5, "Target", Some(0x0100_0800));
    record = record
        .sub(Tag(*b"ALLS"), 6_u32.to_le_bytes())
        .sub(Tag(*b"ALED"), []);
    plugin.push_top(record);
    order.add("QuestPatch.esp", plugin).unwrap();
    Game::new(order)
}

fn payload() -> Vec<u8> {
    let mut out = vec![];
    out.extend(0x8000_0100_u32.to_le_bytes());
    out.extend(0x1234_u16.to_le_bytes());
    out.extend(0x2983_u16.to_le_bytes());
    out.extend(1.25_f32.to_le_bytes());
    out.push(3 << 2);
    for (stage, done) in [(0_u16, 1_u8), (20, 1), (10, 0)] {
        out.extend(stage.to_le_bytes());
        out.push(done);
    }
    out.push(6 << 2);
    for status in 0_u32..6 {
        out.extend(status.to_le_bytes());
        out.extend(status.to_le_bytes());
    }
    out.push(0xA5);
    out.extend(1_u32.to_le_bytes());
    out.extend(5_u32.to_le_bytes());
    out.push(1);
    for raw in [2, 3, 0x400000, 0x400000, 4] {
        reference(raw).write(&mut out);
    }
    out.extend(1_u32.to_le_bytes());
    out.extend(6_u32.to_le_bytes());
    reference(4).write(&mut out);
    out.push(1);
    out.extend(8_u32.to_le_bytes());
    out.extend(b"SCPT");
    out.extend(2_u32.to_le_bytes());
    out.extend(0_u32.to_le_bytes());
    reference(2).write(&mut out);
    out.extend(3_u32.to_le_bytes());
    out.extend(42_u32.to_le_bytes());
    out.extend(7_u32.to_le_bytes());
    out.push(2 << 2);
    for (instance, value) in [(3_u32, -0.0_f32), (7, 9.5)] {
        out.extend(instance.to_le_bytes());
        out.push(1 << 2);
        out.extend(5_u32.to_le_bytes());
        reference(3).write(&mut out);
        out.push(1 << 2);
        reference(3).write(&mut out);
        out.extend(value.to_le_bytes());
        out.extend(20_u16.to_le_bytes());
        out.push(0xFF);
    }
    out.push(1);
    out
}

fn saved() -> Save {
    let mut save = save();
    save.form_id_array = vec![0x0100_0900, 0xFF00_1234, 0x0100_0800, 0xFE00_0800];
    save.change_forms.push(ChangeForm {
        form: reference(1),
        change_flags: change_flags::KNOWN,
        type_field: 8,
        version: 0,
        uncompressed_len: 0,
        data: payload(),
    });
    save
}

fn native(game: &mut Game, target: Value, name: &str, args: &[Value]) -> Value {
    let script = format!("QuestRestore{name}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Var")
        .local("result", "Var");
    let mut operands = vec![s(name), ident("target"), ident("result")];
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, "Var");
        operands.push(ident(&name));
    }
    game.load_script(
        &ScriptBuilder::new(&script, "")
            .function(
                function
                    .ins(Opcode::CallMethod, operands)
                    .ins(Opcode::Return, vec![ident("result")])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    let mut values = vec![target];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn restored_quest_drives_natives_without_replaying_events() {
    let mut game = fixture();
    let report = game.world_mut().restore_vanilla_quests(&saved()).unwrap();
    assert_eq!(report.quests, 1);
    assert_eq!(
        report.unresolved_forms.into_iter().collect::<Vec<_>>(),
        vec![id(DYNAMIC_FILE, 0x1234)]
    );
    assert_eq!(
        report.stage_wait.into_iter().collect::<Vec<_>>(),
        vec![id(3, QUEST)]
    );
    assert!(game.world().pending.is_empty());
    let target = Value::Object(game.object_of_form(id(3, QUEST)));
    for (method, args, expected) in [
        ("GetStage", vec![], Value::Int(20)),
        ("GetCurrentStageID", vec![], Value::Int(20)),
        ("IsStageDone", vec![Value::Int(10)], Value::Bool(false)),
        ("GetStageDone", vec![Value::Int(0)], Value::Bool(true)),
        ("IsCompleted", vec![], Value::Bool(true)),
        ("IsActive", vec![], Value::Bool(true)),
        (
            "IsObjectiveDisplayed",
            vec![Value::Int(5)],
            Value::Bool(true),
        ),
        (
            "IsObjectiveCompleted",
            vec![Value::Int(3)],
            Value::Bool(true),
        ),
        ("IsObjectiveFailed", vec![Value::Int(4)], Value::Bool(true)),
    ] {
        assert_eq!(
            native(&mut game, target.clone(), method, &args),
            expected,
            "{method}"
        );
    }
    let world = game.world();
    let q = world.quest(id(3, QUEST)).unwrap();
    assert_eq!(q.aliases[&5], Some(id(DYNAMIC_FILE, 0x1234)));
    assert_eq!(q.aliases[&6], Some(id(4, 0x800)));
    assert_eq!(q.runtime.other_flags, 0x2180);
    assert_eq!(q.runtime.form_flags, Some((0x8000_0100, 0x1234)));
    assert_eq!(q.runtime.script_delay, Some(1.25));
    assert_eq!(q.runtime.already_run, 1);
    let run = q.runtime.run_data.as_ref().unwrap();
    assert_eq!(run.unknown, 0xA5);
    assert_eq!(
        run.aliases[&5].references,
        vec![Some(id(3, 0x800)), None, None, Some(id(4, 0x800))]
    );
    assert_eq!(run.event.as_ref().unwrap().code, *b"SCPT");
    assert_eq!(run.event.as_ref().unwrap().parameters.len(), 2);
}

#[test]
fn journal_global_updates_select_current_instance_and_persist_history() {
    let mut game = fixture();
    game.world_mut().restore_vanilla_quests(&saved()).unwrap();
    game.world_mut().set_global(id(3, 0x800), 37.0);
    let target = Value::Object(game.object_of_form(id(3, QUEST)));
    let global = Value::Object(game.object_of_form(id(3, 0x800)));
    assert_eq!(
        native(&mut game, target, "UpdateCurrentInstanceGlobal", &[global]),
        Value::Bool(true)
    );
    let bytes = game.world().save_state();
    let hash = game.world().state_hash();
    let restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    assert_eq!(restored.world().state_hash(), hash);
    let world = restored.world();
    let q = world.quest(id(3, QUEST)).unwrap();
    assert_eq!(q.journal.current, 7);
    assert_eq!(q.journal.instances[&7].globals[&id(3, 0x800)], 37.0);
    assert_eq!(
        q.journal.instances[&3].globals[&id(3, 0x800)].to_bits(),
        (-0.0_f32).to_bits()
    );
    assert_eq!(q.journal.instances[&7].aliases[&5], Some(id(3, 0x800)));
    assert_eq!(q.journal.instances[&7].stage, 20);
    assert_eq!(q.journal.instances[&7].stage_item, -1);
}

#[test]
fn late_invalid_quest_keeps_the_entire_game_unchanged() {
    let game = fixture();
    let before = game.save_state().unwrap();
    let mut save = saved();
    let mut late = save.change_forms[0].clone();
    late.form = reference(0x400ABC);
    save.change_forms.push(late);
    assert!(game.world_mut().restore_vanilla_quests(&save).is_err());
    assert_eq!(game.save_state().unwrap(), before);
    assert!(game.world().quest(id(3, QUEST)).is_none());
}

#[test]
fn duplicate_or_invalid_quest_sections_are_atomic_errors() {
    let game = fixture();
    let original = saved();
    for case in 0..7 {
        let mut save = saved();
        let mut change = save.change_forms[0].quest_change().unwrap().unwrap();
        match case {
            0 => change.objectives.as_mut().unwrap().entries[0].status = 6,
            1 => change.runtime.as_mut().unwrap().aliases[0].id = 6,
            2 => change.instances.as_mut().unwrap().unknown = 100,
            3 => change.stages.as_mut().unwrap().entries[0].stage = 123,
            4 => change.stages.as_mut().unwrap().entries[0].status = 2,
            5 => change.objectives.as_mut().unwrap().entries[0].id = 1,
            6 => save.change_forms.push(save.change_forms[0].clone()),
            _ => unreachable!(),
        }
        save.change_forms[0].data = change.data_bytes().unwrap();
        let before = game.save_state().unwrap();
        assert!(
            game.world_mut().restore_vanilla_quests(&save).is_err(),
            "case {case}"
        );
        assert_eq!(game.save_state().unwrap(), before, "case {case}");
    }
    game.world_mut().restore_vanilla_quests(&original).unwrap();
}

#[test]
fn empty_runtime_clears_authored_aliases_and_omitted_sections_keep_defaults() {
    let game = fixture();
    let mut save = saved();
    let mut data = vec![0];
    data.extend(0_u32.to_le_bytes());
    data.extend(0_u32.to_le_bytes());
    data.push(0);
    save.change_forms[0].change_flags = change_flags::RUNTIME;
    save.change_forms[0].data = data;
    game.world_mut().restore_vanilla_quests(&save).unwrap();
    assert_eq!(game.world_mut().alias_ref(id(3, QUEST), 5), None);
    save.change_forms[0].change_flags = 0;
    save.change_forms[0].data.clear();
    game.world_mut().restore_vanilla_quests(&save).unwrap();
    assert_eq!(
        game.world_mut().alias_ref(id(3, QUEST), 5),
        Some(id(3, 0x800))
    );
    assert!(game.world().pending.is_empty());
}

#[test]
fn corrupt_quest_extension_cannot_create_lazy_state() {
    let game = fixture();
    game.world_mut().restore_vanilla_quests(&saved()).unwrap();
    let bytes = game.world().save_state();
    let clean = fixture();
    let before = clean.save_state().unwrap();
    for cut in 1..20 {
        assert!(clean
            .world_mut()
            .load_state(&bytes[..bytes.len() - cut])
            .is_err());
        assert_eq!(clean.save_state().unwrap(), before);
    }
}

#[test]
fn version_ten_current_globals_migrate_to_one_journal_instance() {
    let game = fixture();
    game.world_mut()
        .quest_state(id(3, QUEST))
        .unwrap()
        .journal
        .update_global(id(3, 0x800), -0.0);
    let mut bytes = game.world().save_state();
    // CONTEXT: One quest extension: count, FormID, empty runtime, and one
    // journal instance with a single global and no alias text.
    let extension_bytes = 4 + 6 + 6 + 8 + 4 + 2 + 1 + 4 + 4 + 6 + 4;
    bytes.truncate(bytes.len() - extension_bytes - 21);
    bytes[4..6].copy_from_slice(&10_u16.to_le_bytes());
    let restored = fixture();
    restored.world_mut().load_state(&bytes).unwrap();
    let world = restored.world();
    assert_eq!(
        world
            .quest(id(3, QUEST))
            .unwrap()
            .journal
            .globals()
            .unwrap()[&id(3, 0x800)]
            .to_bits(),
        (-0.0_f32).to_bits()
    );
}

#[test]
fn installed_quests_restore_stages_objectives_aliases_and_all_journal_instances() {
    let (Some(data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping quest restoration corpus");
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
    // CONTEXT: The full supplied corpus has sixty saves and over 32000 quests.
    assert!(paths.len() >= 50);
    let (mut total, mut unresolved, mut waiting) = (0, 0, 0);
    let mut unresolved_union = std::collections::BTreeSet::new();
    for path in &paths {
        let save = Save::open(path).unwrap();
        let mapping = SavedFormMap::new(&save, &order).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        world.restore_vanilla_globals(&save).unwrap();
        let report = world
            .restore_vanilla_quests(&save)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        unresolved += report.unresolved_forms.len();
        unresolved_union.extend(report.unresolved_forms.iter().copied());
        waiting += report.stage_wait.len();
        let mut expected_count = 0;
        for change in &save.change_forms {
            let Some(data) = change.quest_change().unwrap() else {
                continue;
            };
            let id = mapping.reference(change.form).unwrap().unwrap();
            let q = world.quest(id).unwrap();
            if let Some(flags) = data.quest_flags {
                assert_eq!(q.running, flags & 1 != 0);
                assert_eq!(q.completed, flags & 2 != 0);
                assert_eq!(q.failed, flags & 0x40 != 0);
                assert_eq!(q.active, flags & 0x800 != 0);
                assert_eq!(q.runtime.other_flags, flags & !0x843);
            }
            assert_eq!(
                q.runtime.script_delay.map(f32::to_bits),
                data.script_delay.map(f32::to_bits)
            );
            assert_eq!(
                q.runtime.form_flags,
                data.form_flags.map(|f| (f.flags, f.unknown))
            );
            assert_eq!(q.runtime.already_run, data.already_run.unwrap_or(0));
            if let Some(stages) = data.stages {
                for stage in stages.entries {
                    assert_eq!(q.done.contains(&(stage.stage as u16)), stage.status == 1);
                }
            }
            if let Some(objectives) = data.objectives {
                for objective in objectives.entries {
                    let actual = &q.objectives[&(objective.id as u16)];
                    assert_eq!(actual.displayed, objective.status & 1 != 0);
                    assert_eq!(actual.completed, matches!(objective.status, 2 | 3));
                    assert_eq!(actual.failed, matches!(objective.status, 4 | 5));
                }
            }
            if let Some(run) = data.runtime {
                let actual = q.runtime.run_data.as_ref().unwrap();
                assert_eq!(actual.unknown, run.unknown);
                assert_eq!(actual.event_flag, run.event_flag);
                assert_eq!(
                    actual.event.as_ref().map(|e| (e.id, e.code)),
                    run.event.as_ref().map(|e| (e.id, e.code))
                );
                for alias in run.aliases {
                    assert_eq!(
                        q.aliases[&alias.id],
                        mapping.reference(alias.references[0]).unwrap()
                    );
                }
                for alias in run.locations {
                    assert_eq!(
                        q.aliases[&alias.id],
                        mapping.reference(alias.reference).unwrap()
                    );
                }
            }
            if let Some(instances) = data.instances {
                assert_eq!(q.journal.current, instances.unknown);
                assert_eq!(q.journal.instances.len(), instances.instances.entries.len());
                for instance in instances.instances.entries {
                    let actual = &q.journal.instances[&instance.unknown];
                    for global in instance.globals.entries {
                        assert_eq!(
                            actual.globals[&mapping.reference(global.form).unwrap().unwrap()]
                                .to_bits(),
                            global.value.to_bits()
                        );
                    }
                    for alias in instance.aliases.entries {
                        assert_eq!(
                            actual.aliases[&alias.id],
                            mapping.reference(alias.reference).unwrap()
                        );
                    }
                }
            }
            expected_count += 1;
        }
        assert_eq!(report.quests, expected_count);
        assert!(world.pending.is_empty());
        let bytes = world.save_state();
        let mut restored = World::from_shared_order(Arc::clone(&order));
        restored.load_state(&bytes).unwrap();
        let resaved = restored.save_state();
        assert!(
            resaved == bytes,
            "{}: engine save changed at {:?}",
            path.display(),
            resaved.iter().zip(&bytes).position(|(a, b)| a != b)
        );
        assert_eq!(restored.state_hash(), world.state_hash());
        total += report.quests;
    }
    assert!(total > 30000);
    let dynamic = unresolved_union
        .iter()
        .filter(|id| id.file == DYNAMIC_FILE)
        .count();
    let statics: Vec<_> = unresolved_union
        .iter()
        .filter(|id| id.file != DYNAMIC_FILE)
        .collect();
    eprintln!("Unresolved identities across corpus: {dynamic} unique created forms, {} plugin forms: {:?}", statics.len(), &statics[..statics.len().min(12)]);
    eprintln!("{} saves: {total} live quests, {unresolved} unresolved world identities, {waiting} stage-wait quests", paths.len());
}
