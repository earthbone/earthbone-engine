// SPDX-License-Identifier: GPL-3.0-or-later

//! NPC changes must reach shared records and actors across file namespaces.

use super::*;
use soe_game::npc::{self, Config, Stats};
use soe_save::npc::flags as saved_flags;

fn fixture_order() -> LoadOrder {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(npc::build_record(
        0x800,
        "TestNPC",
        "Original",
        Config::default(),
        Stats::default(),
    ));
    base.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    base.push_top(Record::new(Tag(*b"RACE"), 0x820));
    base.push_top(Record::new(Tag(*b"PACK"), 0x840));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order
        .add("B.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut a = Plugin::new(FileHeader::default());
    for (object, tag) in [
        (0x830, *b"CLAS"),
        (0x831, *b"SPEL"),
        (0x834, *b"SHOU"),
        (0x83A, *b"SPEL"),
    ] {
        a.push_top(Record::new(Tag(tag), object));
    }
    let mut entry = vec![0; 12];
    entry[..2].copy_from_slice(&1u16.to_le_bytes());
    entry[4..8].copy_from_slice(&0x83Au32.to_le_bytes());
    entry[8..10].copy_from_slice(&1u16.to_le_bytes());
    a.push_top(
        Record::new(Tag(*b"LVSP"), 0x836)
            .sub(Tag(*b"LLCT"), [1])
            .sub(Tag(*b"LVLO"), entry),
    );
    order.add("A.esp", a).unwrap();
    order
        .add(
            "Extra.esl",
            Plugin::new(FileHeader {
                flags: flags::LIGHT_MASTER,
                ..Default::default()
            }),
        )
        .unwrap();
    let mut light = Plugin::new(FileHeader {
        flags: flags::LIGHT_MASTER,
        ..Default::default()
    });
    for (object, tag) in [
        (0x832, *b"FACT"),
        (0x833, *b"RACE"),
        (0x835, *b"OTFT"),
        (0x837, *b"CLFM"),
        (0x838, *b"TXST"),
        (0x839, *b"HDPT"),
    ] {
        light.push_top(Record::new(Tag(tag), object));
    }
    order.add("Light.esl", light).unwrap();
    let mut patch = Plugin::new(FileHeader {
        flags: flags::LOCALIZED,
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    let mut record = npc::build_record(
        0x800,
        "TestNPC",
        "Unused",
        Config::default(),
        Stats::default(),
    );
    record.subrecords.retain(|sub| sub.tag != Tag(*b"FULL"));
    record = record
        .sub(Tag(*b"FULL"), 123_u32.to_le_bytes())
        .sub(Tag(*b"RNAM"), 0x820_u32.to_le_bytes())
        .sub(Tag(*b"PKID"), 0x840_u32.to_le_bytes())
        .sub(Tag(*b"UNKN"), [9, 255, 255, 255, 255, 8]);
    patch.push_top(record);
    order.add("Patch.esp", patch).unwrap();
    order
}

fn fixture() -> Save {
    let mut saved = save();
    // Saved and live positions differ for both A.esp and Light.esl.
    saved.form_id_array = vec![
        0x01000830, 0x01000831, 0xFE000832, 0xFE000833, 0x01000834, 0xFE000835, 0x01000836,
        0xFE000837, 0xFE000838, 0xFE000839,
    ];
    let record = npc::build_record(
        0x800,
        "TestNPC",
        "Unused",
        Config {
            flags: npc::flags::AUTO_CALC_STATS | npc::flags::ESSENTIAL,
            level: 27,
            ..Config::default()
        },
        Stats {
            health: 777,
            magicka: 222,
            stamina: 333,
            ..Stats::default()
        },
    );
    let flags = saved_flags::BASE
        | saved_flags::FACTIONS
        | saved_flags::SPELLS
        | saved_flags::AI
        | saved_flags::NAME
        | saved_flags::SKILLS
        | saved_flags::CLASS
        | saved_flags::RACE
        | saved_flags::FACE
        | saved_flags::GENDER
        | saved_flags::DEFAULT_OUTFIT
        | saved_flags::SLEEP_OUTFIT;
    let mut data = record.find(Tag(*b"ACBS")).unwrap().to_vec();
    data.push(4);
    reference(3).write(&mut data);
    data.push(255);
    for form in [2, 7, 5] {
        data.push(4);
        reference(form).write(&mut data);
    }
    data.extend([2, 3, 50, 1, 4, 2, 1, 0]);
    for radius in [500_u32, 300, 100] {
        data.extend(radius.to_le_bytes());
    }
    WString::from_text("Éowyn €").write(&mut data);
    data.extend(record.find(Tag(*b"DNAM")).unwrap());
    reference(1).write(&mut data);
    reference(4).write(&mut data);
    reference(0x400820).write(&mut data);
    data.push(1);
    reference(8).write(&mut data);
    data.extend([255, 128, 0, 17]);
    reference(9).write(&mut data);
    data.push(4);
    reference(10).write(&mut data);
    data.push(1);
    data.extend(2_u32.to_le_bytes());
    data.extend(0x80000000_u32.to_le_bytes());
    data.extend(0x7FC01234_u32.to_le_bytes());
    data.extend(1_u32.to_le_bytes());
    data.extend(9_u32.to_le_bytes());
    data.push(1);
    reference(6).write(&mut data);
    reference(0).write(&mut data);
    saved.change_forms = vec![soe_save::ChangeForm {
        form: reference(0x400800),
        change_flags: flags,
        type_field: 9,
        version: 0,
        uncompressed_len: 0,
        data,
    }];
    saved
}

fn assert_actor(world: &mut World) {
    assert_eq!(world.name_of(id(0, 0x800)).as_deref(), Some("Éowyn €"));
    assert_eq!(world.name_of(id(0, 0x900)).as_deref(), Some("Éowyn €"));
    let base = world.actor_base(id(0, 0x800)).unwrap();
    assert!(base.essential);
    assert!(base.female);
    assert_eq!(base.race, Some(id(4, 0x833)));
    assert_eq!(base.class, Some(id(2, 0x830)));
    assert_eq!(base.outfit, Some(id(4, 0x835)));
    assert_eq!(base.sleep_outfit, None);
    let record = world.record(id(0, 0x800)).unwrap();
    assert!(record.all(Tag(*b"SPLO")).any(|raw| {
        world.resolve_from(id(0, 0x800), u32::from_le_bytes(raw.try_into().unwrap()))
            == Some(id(2, 0x836))
    }));
    let actor = world.actor(id(0, 0x900)).unwrap();
    assert_eq!(actor.values.get("health").base(), 777.0);
    for (name, expected) in [
        ("aggression", 2.0),
        ("confidence", 3.0),
        ("energy", 50.0),
        ("morality", 1.0),
        ("mood", 4.0),
        ("assistance", 2.0),
    ] {
        assert_eq!(actor.values.get(name).base(), expected, "{name}");
    }
    assert_eq!(actor.level, 27);
    assert!(actor.essential);
    assert_eq!(actor.race, Some(id(4, 0x833)));
    assert_eq!(actor.factions[&id(4, 0x832)], -1);
    assert!(actor.spells.contains(&id(2, 0x831)));
    assert!(actor.spells.contains(&id(2, 0x83A)));
    assert!(!actor.spells.contains(&id(2, 0x836)));
    assert!(!actor.spells.contains(&id(2, 0x834)));
    assert!(actor.shouts.contains(&id(2, 0x834)));
    assert_eq!(actor.sleep_outfit, None);
}

#[test]
fn restored_npc_fields_reach_records_actors_and_native_queries() {
    let mut game = Game::new(fixture_order());
    let saved = fixture();
    assert_eq!(
        game.world().name_of(id(0, 0x800)).as_deref(),
        Some("<string 123>")
    );
    let report = game.world_mut().restore_vanilla_npcs(&saved).unwrap();
    assert_eq!(report.bases, 1);
    assert!(report.missing_links.is_empty());
    assert!(report.pending_form_flags.is_empty());
    let world = game.world();
    let record = world.record(id(0, 0x800)).unwrap();
    assert_eq!(
        record.find(Tag(*b"UNKN")).unwrap(),
        [9, 255, 255, 255, 255, 8]
    );
    assert_eq!(world.resolve_from(id(0, 0x800), 0x840), Some(id(0, 0x840)));
    assert_eq!(
        world.resolve_from(id(0, 0x800), u32::MAX),
        Some(id(5, 0xFFFFFF))
    );
    let extra = world.restored_npc_extra(id(0, 0x800)).unwrap();
    assert_eq!(extra.original_race, Some(id(0, 0x820)));
    assert_eq!(extra.body_tint, Some([255, 128, 0, 17]));
    assert_eq!(
        record.find(Tag(*b"NAM9")).unwrap()[4..],
        0x7FC01234_u32.to_le_bytes()
    );
    assert_eq!(
        world
            .order()
            .winner(id(0, 0x800))
            .unwrap()
            .find(Tag(*b"FULL"))
            .unwrap(),
        123_u32.to_le_bytes()
    );
    drop(world);
    assert_actor(&mut game.world_mut());
    let script = ScriptBuilder::new("NpcQuery", "Form")
        .function(
            FunctionBuilder::new("Read", "Float")
                .global()
                .param("actor", "Actor")
                .local("health", "Float")
                .ins(
                    Opcode::CallMethod,
                    vec![
                        s("GetBaseActorValue"),
                        ident("actor"),
                        ident("health"),
                        s("Health"),
                    ],
                )
                .ins(Opcode::Return, vec![ident("health")])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let actor = game.object_of_form(id(0, 0x900));
    let thread = game
        .vm
        .call_global("NpcQuery", "Read", &[Value::Object(actor)])
        .unwrap();
    game.run_until_idle(64);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Float(777.0)));
}

#[test]
fn record_overrides_persist_before_actor_initialization_and_old_saves_clear_them() {
    let mut world = World::new(fixture_order());
    let empty = world.save_state();
    let saved = fixture();
    world.restore_vanilla_npcs(&saved).unwrap();
    assert!(world.loaded_actors().is_empty());
    let before = world.save_state();
    let hash = world.state_hash();
    let mut v13 = before.clone();
    // `empty` carries the same 6 bytes of unconditional tail growth as
    // `before` past record overrides (detection's now-unconditional empty
    // pool count and the world-level save-section table version 37 added),
    // so the offset grows by 6 to still land on record overrides' start.
    let origin_fields = empty.len() - 42;
    v13.drain(origin_fields..origin_fields + 13);
    // CONTEXT: Version 13 has no list metadata byte in record overrides.
    v13.remove(origin_fields + 14);
    // The extra 6 bytes beyond the old 42 are the same detection and
    // world-level save-section growth, always present at the tail
    // regardless of the simulated version.
    v13.truncate(v13.len() - 48);
    v13[4..6].copy_from_slice(&13_u16.to_le_bytes());
    let mut legacy = World::new(fixture_order());
    legacy.load_state(&v13).unwrap();
    assert_eq!(legacy.save_state(), before);
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&before).unwrap();
    assert_eq!(loaded.state_hash(), hash);
    assert_eq!(loaded.save_state(), before);
    assert_actor(&mut loaded);
    assert_actor(&mut world);
    assert_eq!(loaded.state_hash(), world.state_hash());
    for cut in [1, 2, 4, 10, 20, 100] {
        let stable = loaded.save_state();
        assert!(loaded.load_state(&before[..before.len() - cut]).is_err());
        assert_eq!(loaded.save_state(), stable);
    }
    let mut v12 = empty;
    // Same 6-byte tail growth as v13 above, past the old 46.
    v12.truncate(v12.len() - 52);
    v12[4..6].copy_from_slice(&12_u16.to_le_bytes());
    loaded.load_state(&v12).unwrap();
    assert!(loaded.restored_npc_extra(id(0, 0x800)).is_none());
    assert_eq!(
        loaded.name_of(id(0, 0x800)).as_deref(),
        Some("<string 123>")
    );
}

#[test]
fn late_invalid_npc_changes_and_wrong_phase_do_not_partially_apply() {
    let original = fixture();
    for kind in 0..5 {
        let mut saved = original.clone();
        let mut late = saved.change_forms[0].clone();
        match kind {
            0 => late.form = reference(0x400999),
            1 => {
                late.data.pop();
            }
            2 => late.change_flags |= 1 << 31,
            3 => late.form = reference(0x800999),
            _ => {}
        }
        saved.change_forms.push(late);
        let mut world = World::new(fixture_order());
        let before = world.save_state();
        assert!(world.restore_vanilla_npcs(&saved).is_err());
        assert_eq!(world.save_state(), before);
        world.restore_vanilla_npcs(&original).unwrap();
    }
    for actor in [true, false] {
        let mut world = World::new(fixture_order());
        if actor {
            world.actor(id(0, 0x900));
        } else {
            world.actor_base(id(0, 0x800));
        }
        let before = world.save_state();
        assert!(world.restore_vanilla_npcs(&original).is_err());
        assert_eq!(world.save_state(), before);
    }
}

#[test]
fn unresolved_links_and_unapplied_form_masks_are_retained_and_reported() {
    let mut saved = fixture();
    saved.form_id_array[2] = 0xFE000AAA;
    let change = &mut saved.change_forms[0];
    change.change_flags |= saved_flags::FORM;
    change.data.splice(..0, [1, 2, 3, 4, 5, 6]);
    let mut world = World::new(fixture_order());
    let report = world.restore_vanilla_npcs(&saved).unwrap();
    assert!(report.missing_links.contains(&id(4, 0xAAA)));
    assert_eq!(
        report.pending_form_flags[&id(0, 0x800)],
        (0x04030201, 0x0605)
    );
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(
        loaded.restored_npc_extra(id(0, 0x800)),
        world.restored_npc_extra(id(0, 0x800))
    );
    assert_eq!(loaded.state_hash(), world.state_hash());
}

#[test]
fn installed_npc_changes_restore_into_shared_records_and_persist() {
    let (Some(data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
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
    assert!(paths.len() >= 50);
    let mut total = 0;
    let mut missing = std::collections::BTreeSet::new();
    for path in &paths {
        let save = Save::open(path).unwrap();
        let map = SavedFormMap::new(&save, &order).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        let report = world
            .restore_vanilla_npcs(&save)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(report.pending_form_flags.is_empty());
        missing.extend(report.missing_links);
        let expected = save
            .change_forms
            .iter()
            .filter(|f| f.type_index() == 9)
            .count();
        assert_eq!(report.bases, expected);
        for change in &save.change_forms {
            let Some(npc) = change.npc_change().unwrap() else {
                continue;
            };
            let id = map.reference(change.form).unwrap().unwrap();
            let record = world.record(id).unwrap();
            if let Some(base) = npc.base {
                assert!(npc.gender.is_none());
                assert_eq!(record.find(Tag(*b"ACBS")).unwrap(), base);
            }
            if let Some(skills) = npc.skills {
                assert_eq!(record.find(Tag(*b"DNAM")).unwrap(), skills);
            }
            if let Some(ai) = npc.ai {
                assert_eq!(record.find(Tag(*b"AIDT")).unwrap(), ai);
            }
            if let Some(name) = npc.name {
                assert_eq!(
                    world.name_of(id).unwrap(),
                    name.to_text().trim_end_matches('\0')
                );
            }
            if let Some(factions) = npc.factions {
                let entries: Vec<_> = record
                    .all(Tag(*b"SNAM"))
                    .map(|v| {
                        (
                            world.resolve_from(id, u32::from_le_bytes(v[..4].try_into().unwrap())),
                            v[4] as i8,
                        )
                    })
                    .collect();
                let expected: Vec<_> = factions
                    .entries
                    .iter()
                    .map(|e| (map.reference(e.form).unwrap(), e.rank))
                    .collect();
                assert_eq!(entries, expected);
            }
        }
        assert!(world.loaded_actors().is_empty());
        assert!(world.pending.is_empty());
        let hash = world.state_hash();
        let mut loaded = World::from_shared_order(Arc::clone(&order));
        loaded.load_state(&world.save_state()).unwrap();
        assert_eq!(loaded.state_hash(), hash);
        for change in save.change_forms.iter().filter(|f| f.type_index() == 9) {
            let id = map.reference(change.form).unwrap().unwrap();
            assert_eq!(loaded.record(id), world.record(id));
            assert_eq!(loaded.restored_npc_extra(id), world.restored_npc_extra(id));
        }
        assert_eq!(world.random_u64(), loaded.random_u64());
        total += expected;
    }
    eprintln!(
        "{} saves: {total} NPC bases restored and persisted, missing links={missing:?}",
        paths.len()
    );
    assert!(total > 10000);
    assert!(missing.is_empty());
}
