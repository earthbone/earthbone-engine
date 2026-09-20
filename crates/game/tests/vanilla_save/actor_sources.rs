// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved choices preserve source identity without consuming random state.

use super::*;

fn fixture_order() -> LoadOrder {
    use soe_game::npc::{self, template, Config, Stats};
    let mut plugin = Plugin::new(FileHeader::default());
    let player = npc::build_record(7, "Player", "Player", Config::default(), Stats::default());
    let original = npc::build_record(
        0x810,
        "Original",
        "Original",
        Config {
            template_flags: template::INVENTORY,
            ..Config::default()
        },
        Stats::default(),
    )
    .sub(Tag(*b"TPLT"), 0x811_u32.to_le_bytes());
    let mut cnto = 0x813_u32.to_le_bytes().to_vec();
    cnto.extend(3_i32.to_le_bytes());
    let selected = npc::build_record(
        0x812,
        "Selected",
        "Selected",
        Config::default(),
        Stats::default(),
    )
    .sub(Tag(*b"CNTO"), cnto);
    let mut lvlo = vec![0; 12];
    lvlo[..2].copy_from_slice(&99_u16.to_le_bytes());
    lvlo[4..8].copy_from_slice(&0x812_u32.to_le_bytes());
    lvlo[8..10].copy_from_slice(&1_u16.to_le_bytes());
    let list = Record::new(Tag(*b"LVLN"), 0x811)
        .sub(Tag(*b"LVLO"), lvlo)
        .sub(Tag(*b"LVLD"), [100]);
    for record in [
        player,
        original,
        selected,
        list,
        Record::new(Tag(*b"MISC"), 0x813),
        Record::new(Tag(*b"CELL"), 0x800),
    ] {
        plugin.push_top(record);
    }
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(actor: u32, selected: u32) -> soe_save::ChangeForm {
    let mut data = vec![0; 8];
    data.extend([4, 45]);
    reference(0x400810).write(&mut data);
    reference(selected).write(&mut data);
    data.extend(0_u32.to_le_bytes());
    soe_save::ChangeForm {
        form: reference(actor),
        type_field: 1,
        version: 0,
        change_flags: 1 << 18,
        data,
        uncompressed_len: 0,
    }
}

fn fixture() -> Save {
    let mut saved = save();
    saved.plugins.truncate(1);
    saved.light_plugins.clear();
    saved.table_1.push(GlobalData {
        kind: 1,
        data: soe_save::reference::PlayerLocation {
            next_object_id: 0xFF000800,
            world: reference(0),
            coordinates: [0; 2],
            cell_or_world: reference(0x400800),
            position: [0.0; 3],
            unknown: 0,
        }
        .data_bytes(),
    });
    saved.change_forms = vec![
        soe_save::ChangeForm {
            form: reference(0x400014),
            type_field: 1,
            version: 0,
            change_flags: 0,
            data: vec![0; 8],
            uncompressed_len: 0,
        },
        change(0x400900, 0x400812),
        change(0x400901, 0x400812),
    ];
    saved
}

#[test]
fn restored_choices_ignore_current_level_and_chance_and_supply_saved_inventory() {
    let saved = fixture();
    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&saved).unwrap();
    let before = world.save_state();
    let report = world.restore_vanilla_actor_templates(&saved).unwrap();
    assert_eq!(report.actors, 2);
    assert_eq!(report.restored_npc_changes, 0);
    assert!(report.pending_form_flags.is_empty());
    let temporary = world.leveled_actor_base(id(0, 0x900)).unwrap().unwrap();
    let other = world.leveled_actor_base(id(0, 0x901)).unwrap().unwrap();
    assert_ne!(temporary, other);
    assert_eq!(
        world.from_runtime_id(world.runtime_id(temporary)),
        Some(temporary)
    );
    world.player_level = 1;
    assert_eq!(
        world
            .actor_component_source(id(0, 0x900), soe_game::npc::template::INVENTORY)
            .unwrap(),
        temporary
    );
    world.restore_vanilla_inventories(&saved).unwrap();
    assert_eq!(world.item_count(id(0, 0x900), id(0, 0x813)), 3);
    let mut control = World::new(fixture_order());
    control.load_state(&before).unwrap();
    assert_eq!(world.random_u64(), control.random_u64());
    assert!(world.pending.is_empty());
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(loaded.state_hash(), world.state_hash());
    assert_eq!(
        loaded
            .actor_component_source(id(0, 0x901), soe_game::npc::template::INVENTORY)
            .unwrap(),
        other
    );
}

#[test]
fn a_late_invalid_choice_cannot_install_partial_template_state() {
    let original = fixture();
    for kind in 0..4 {
        let mut world = World::new(fixture_order());
        world
            .restore_vanilla_reference_placements(&original)
            .unwrap();
        let before = world.save_state();
        let mut saved = original.clone();
        match kind {
            0 => saved.change_forms[2] = change(0x400901, 0x400813),
            1 => saved.change_forms[2] = change(0x400901, 0x400007),
            2 => saved.change_forms[2] = change(0x400900, 0x400812),
            _ => {
                saved.change_forms[2].data.pop();
            }
        }
        assert!(world.restore_vanilla_actor_templates(&saved).is_err());
        assert_eq!(world.save_state(), before);
        world.restore_vanilla_actor_templates(&original).unwrap();
    }
}

#[test]
fn inline_npc_changes_are_private_and_unknown_selection_level_is_not_written_as_player_level() {
    let mut saved = fixture();
    let actor = &mut saved.change_forms[1];
    let at = actor.data.len() - 4;
    actor.data[at..].copy_from_slice(&2_u32.to_le_bytes());
    actor.data.extend([0; 24]);
    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&saved).unwrap();
    let report = world.restore_vanilla_actor_templates(&saved).unwrap();
    assert_eq!(report.restored_npc_changes, 1);
    let temporary = world.leveled_actor_base(id(0, 0x900)).unwrap().unwrap();
    assert_eq!(
        world
            .record(temporary)
            .unwrap()
            .find(Tag(*b"ACBS"))
            .unwrap(),
        [0; 24]
    );
    assert_ne!(
        world
            .record(id(0, 0x810))
            .unwrap()
            .find(Tag(*b"ACBS"))
            .unwrap(),
        [0; 24]
    );
    let bytes = world.save_state();
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.state_hash(), world.state_hash());
    let mut old = bytes;
    old.truncate(old.len() - 17);
    old[4..6].copy_from_slice(&11_u16.to_le_bytes());
    assert!(
        loaded.load_state(&old).is_err(),
        "v11 did not define an unknown encounter level"
    );
}

#[test]
fn installed_saved_actor_choices_restore_without_rerolling_and_persist() {
    let (Some(data), Some(corpus_dir)) = (
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
    collect_saves(Path::new(&corpus_dir), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut total, mut restored) = (0, 0);
    for path in &paths {
        let save = Save::open(path).unwrap();
        let mapping = SavedFormMap::new(&save, &order).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        world.restore_vanilla_npcs(&save).unwrap();
        world.restore_vanilla_reference_placements(&save).unwrap();
        let before = world.save_state();
        let report = world
            .restore_vanilla_actor_templates(&save)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        total += report.actors;
        restored += report.restored_npc_changes;
        assert!(report.pending_form_flags.is_empty());
        let mut count = 0;
        for change in &save.change_forms {
            let Some(saved) = change.leveled_creature(save.form_version).unwrap() else {
                continue;
            };
            let actor = mapping.reference(change.form).unwrap().unwrap();
            let temporary = world.leveled_actor_base(actor).unwrap().unwrap();
            assert_ne!(world.base_of(actor), Some(temporary));
            let record = world.record(temporary).unwrap();
            for (tag, bytes) in [
                (Tag(*b"ACBS"), saved.npc.base.as_ref().map(|v| v.as_slice())),
                (Tag(*b"AIDT"), saved.npc.ai.as_ref().map(|v| v.as_slice())),
                (
                    Tag(*b"DNAM"),
                    saved.npc.skills.as_ref().map(|v| v.as_slice()),
                ),
            ] {
                if let Some(bytes) = bytes {
                    assert_eq!(record.find(tag), Some(bytes));
                }
            }
            if let (Some(base), Some(stats)) = (saved.npc.base, saved.npc.skills) {
                if u32::from_le_bytes(base[..4].try_into().unwrap())
                    & soe_game::npc::flags::AUTO_CALC_STATS
                    != 0
                {
                    let health = u16::from_le_bytes([stats[36], stats[37]]);
                    assert_eq!(world.base_actor_value(actor, "health"), f32::from(health));
                }
            }
            world
                .actor_component_source(actor, soe_game::npc::template::INVENTORY)
                .unwrap();
            count += 1;
        }
        assert_eq!(count, report.actors);
        assert!(world.pending.is_empty());
        let mut control = World::from_shared_order(Arc::clone(&order));
        control.load_state(&before).unwrap();
        assert_eq!(world.random_u64(), control.random_u64());
        let mut loaded = World::from_shared_order(Arc::clone(&order));
        loaded.load_state(&world.save_state()).unwrap();
        assert_eq!(loaded.state_hash(), world.state_hash());
    }
    assert!(total > 10000);
    assert!(restored > 4000);
    eprintln!(
        "{} saves: {total} temporary actor bases, {restored} inline NPC changes applied",
        paths.len()
    );
}

fn with_stats(mut form: soe_save::ChangeForm, health: u16, level: u16) -> soe_save::ChangeForm {
    use soe_game::npc::{self, Config, Stats};
    let record = npc::build_record(
        0,
        "",
        "",
        Config {
            flags: npc::flags::AUTO_CALC_STATS | npc::flags::FEMALE | npc::flags::ESSENTIAL,
            level,
            template_flags: 0x9fff,
            ..Config::default()
        },
        Stats {
            health,
            ..Stats::default()
        },
    );
    let at = form.data.len() - 4;
    form.data[at..].copy_from_slice(&0x20A_u32.to_le_bytes());
    form.data.extend(record.find(Tag(*b"ACBS")).unwrap());
    form.data.extend([0; 20]);
    form.data.extend(record.find(Tag(*b"DNAM")).unwrap());
    form
}

#[test]
fn restored_stats_and_native_base_identity_are_local_to_each_actor() {
    let mut saved = fixture();
    saved.change_forms[1] = with_stats(saved.change_forms[1].clone(), 321, 17);
    saved.change_forms[2] = with_stats(saved.change_forms[2].clone(), 678, 29);
    let mut game = Game::new(fixture_order());
    game.world_mut()
        .restore_vanilla_reference_placements(&saved)
        .unwrap();
    let before = game.world().record(id(0, 0x810)).unwrap().clone();
    let selected = game.world().record(id(0, 0x812)).unwrap().clone();
    let report = game
        .world_mut()
        .restore_vanilla_actor_templates(&saved)
        .unwrap();
    assert_eq!(report.restored_npc_changes, 2);
    let mut temporary = vec![];
    for (actor, health, level) in [(0x900, 321.0, 17), (0x901, 678.0, 29)] {
        let mut world = game.world_mut();
        let base = world.leveled_actor_base(id(0, actor)).unwrap().unwrap();
        temporary.push(base);
        assert_eq!(base.file, DYNAMIC_FILE);
        assert_eq!(world.base_of(id(0, actor)), Some(id(0, 0x810)));
        assert_eq!(world.base_actor_value(id(0, actor), "health"), health);
        assert_eq!(world.actor(id(0, actor)).unwrap().level, level);
        assert!(world.actor_base(base).unwrap().female);
        assert!(world.actor(id(0, actor)).unwrap().essential);
    }
    assert_ne!(temporary[0], temporary[1]);
    assert_eq!(game.world().record(id(0, 0x810)), Some(&before));
    assert_eq!(game.world().record(id(0, 0x812)), Some(&selected));
    let mut builder = ScriptBuilder::new("TemporaryNpcQuery", "Form");
    for (function, native) in [
        ("Original", "GetActorBase"),
        ("Temporary", "GetLeveledActorBase"),
    ] {
        builder = builder.function(
            FunctionBuilder::new(function, "ActorBase")
                .global()
                .param("actor", "Actor")
                .local("base", "ActorBase")
                .ins(
                    Opcode::CallMethod,
                    vec![s(native), ident("actor"), ident("base")],
                )
                .ins(Opcode::Return, vec![ident("base")])
                .build(),
        );
    }
    game.load_script(&builder.bytes()).unwrap();
    let actor = game.object_of_form(id(0, 0x900));
    for (function, expected) in [("Original", id(0, 0x810)), ("Temporary", temporary[0])] {
        let thread = game
            .vm
            .call_global("TemporaryNpcQuery", function, &[Value::Object(actor)])
            .unwrap();
        game.run_until_idle(64);
        let Some(Value::Object(object)) = game.vm.thread_result(thread) else {
            panic!("missing base object")
        };
        assert_eq!(game.world().form_of(object), Some(expected));
    }
    let bytes = game.world().save_state();
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state(), bytes);
    for (actor, health) in [(0x900, 321.0), (0x901, 678.0)] {
        assert_eq!(loaded.base_actor_value(id(0, actor), "health"), health);
    }
    // The saved change-form order cannot influence allocated base identities.
    saved.change_forms.swap(1, 2);
    let mut reordered = World::new(fixture_order());
    reordered
        .restore_vanilla_reference_placements(&saved)
        .unwrap();
    reordered.restore_vanilla_actor_templates(&saved).unwrap();
    assert_eq!(
        reordered.leveled_actor_base(id(0, 0x900)).unwrap(),
        Some(temporary[0])
    );
    assert_eq!(
        reordered.leveled_actor_base(id(0, 0x901)).unwrap(),
        Some(temporary[1])
    );
}

#[test]
fn a_bad_temporary_base_payload_does_not_allocate_or_install_earlier_bases() {
    let mut saved = fixture();
    // A valid first change is followed by an invalid saved gender in the second.
    saved.change_forms[1] = with_stats(saved.change_forms[1].clone(), 321, 17);
    let at = saved.change_forms[2].data.len() - 4;
    saved.change_forms[2].data[at..].copy_from_slice(&(1_u32 << 24).to_le_bytes());
    saved.change_forms[2].data.push(7);
    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&saved).unwrap();
    let before = world.save_state();
    assert!(world.restore_vanilla_actor_templates(&saved).is_err());
    assert_eq!(world.save_state(), before);
    saved.change_forms[2].data[at..].fill(0);
    saved.change_forms[2].data.truncate(at + 4);
    world.restore_vanilla_actor_templates(&saved).unwrap();
    assert_eq!(world.base_actor_value(id(0, 0x900), "health"), 321.0);
    let good = world.save_state();
    for cut in [1, 4, 20, 100] {
        assert!(world.load_state(&good[..good.len() - cut]).is_err());
        assert_eq!(world.save_state(), good);
    }
}

fn namespace_order() -> LoadOrder {
    use soe_game::npc::{self, template, Config, Stats};
    use soe_game::vmad::{ObjectRef, PropertyValue, ScriptEntry, Vmad};
    let mut base = Plugin::new(FileHeader::default());
    let root = npc::build_record(
        0x810,
        "Root",
        "Root",
        Config {
            flags: npc::flags::FEMALE,
            template_flags: template::STATS
                | template::SPELLS
                | template::INVENTORY
                | template::SCRIPT
                | template::AI_DATA
                | template::KEYWORDS
                | template::DEF_PACK_LIST
                | template::FACTIONS
                | template::ATTACK_DATA,
            ..Config::default()
        },
        Stats::default(),
    )
    .sub(Tag(*b"TPLT"), 0x811_u32.to_le_bytes())
    .sub(Tag(*b"RNAM"), 0x830_u32.to_le_bytes());
    for record in [
        npc::build_record(7, "Player", "Player", Config::default(), Stats::default()),
        root,
        npc::build_record(
            0x812,
            "Selected",
            "Selected",
            Config::default(),
            Stats::default(),
        ),
        Record::new(Tag(*b"RACE"), 0x830),
        Record::new(Tag(*b"CELL"), 0x800),
        Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()),
        Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()),
    ] {
        base.push_top(record);
    }
    let mut lvlo = vec![0; 12];
    lvlo[4..8].copy_from_slice(&0x812_u32.to_le_bytes());
    lvlo[8..10].copy_from_slice(&1_u16.to_le_bytes());
    base.push_top(Record::new(Tag(*b"LVLN"), 0x811).sub(Tag(*b"LVLO"), lvlo));
    let mut other = Plugin::new(FileHeader::default());
    for (tag, object) in [
        (b"CLAS", 0x830),
        (b"SPEL", 0x831),
        (b"MISC", 0x832),
        (b"GLOB", 0x833),
        (b"KYWD", 0x834),
        (b"FACT", 0x835),
        (b"CSTY", 0x836),
        (b"FLST", 0x837),
    ] {
        other.push_top(Record::new(Tag(*tag), object));
    }
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into(), "Other.esm".into()],
        ..FileHeader::default()
    });
    let mut selected = npc::build_record(
        0x812,
        "Selected",
        "Selected",
        Config {
            flags: npc::flags::AUTO_CALC_STATS,
            level: 23,
            ..Config::default()
        },
        Stats {
            health: 432,
            ..Stats::default()
        },
    );
    for (tag, raw) in [
        (b"CNAM", 0x01000830_u32),
        (b"SPLO", 0x01000831),
        (b"ZNAM", 0x01000836),
        (b"DPLT", 0x01000837),
    ] {
        selected = selected.sub(Tag(*tag), raw.to_le_bytes());
    }
    selected = selected
        .sub(
            Tag(*b"CNTO"),
            [0x01000832_u32.to_le_bytes(), 3_u32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"KWDA"), 0x01000834_u32.to_le_bytes())
        .sub(
            Tag(*b"SNAM"),
            [0x01000835_u32.to_le_bytes(), [2, 0, 0, 0]].concat(),
        )
        .sub(
            Tag(*b"COED"),
            [
                0x01000835_u32.to_le_bytes(),
                7_u32.to_le_bytes(),
                1_f32.to_le_bytes(),
            ]
            .concat(),
        );
    let mut attack = vec![0; 44];
    attack[8..12].copy_from_slice(&0x01000831_u32.to_le_bytes());
    attack[28..32].copy_from_slice(&0x01000834_u32.to_le_bytes());
    selected = selected
        .sub(Tag(*b"ATKD"), attack)
        .zsub(Tag(*b"ATKE"), "attackStart");
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("Test").prop(
            "Link",
            PropertyValue::Object(ObjectRef {
                form_id: 0x01000833,
                alias: -1,
            }),
        )],
        ..Vmad::default()
    };
    selected = selected.sub(Tag(*b"VMAD"), vmad.write(Tag(*b"NPC_")));
    patch.push_top(selected);
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", base).unwrap();
    order.add("Other.esm", other).unwrap();
    order.add("Patch.esp", patch).unwrap();
    order
}

#[test]
fn temporary_components_keep_their_source_namespace_in_native_and_opaque_fields() {
    use soe_game::vmad::{PropertyValue, Vmad};
    let mut saved = fixture();
    saved.plugins.extend([
        WString::from_text("Other.esm"),
        WString::from_text("Patch.esp"),
    ]);
    let mut world = World::new(namespace_order());
    world.restore_vanilla_reference_placements(&saved).unwrap();
    world.restore_vanilla_actor_templates(&saved).unwrap();
    let temp = world.leveled_actor_base(id(0, 0x900)).unwrap().unwrap();
    let record = world.record(temp).unwrap();
    let read = |tag: &[u8; 4], at: usize| {
        let bytes = record.find(Tag(*tag)).unwrap();
        let raw = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        world.resolve_from(temp, raw).unwrap()
    };
    assert_eq!(read(b"RNAM", 0), id(0, 0x830));
    assert_eq!(read(b"CNAM", 0), id(1, 0x830));
    assert_eq!(read(b"CNTO", 0), id(1, 0x832));
    assert_eq!(read(b"SNAM", 0), id(1, 0x835));
    assert_eq!(read(b"ATKD", 8), id(1, 0x831));
    assert_eq!(read(b"ATKD", 28), id(1, 0x834));
    assert_eq!(read(b"COED", 0), id(1, 0x835));
    assert_eq!(
        &record.find(Tag(*b"COED")).unwrap()[4..8],
        &7_u32.to_le_bytes()
    );
    let vmad = Vmad::parse(record.find(Tag(*b"VMAD")).unwrap(), Tag(*b"NPC_")).unwrap();
    let PropertyValue::Object(link) = vmad.scripts[0].properties[0].value else {
        panic!("missing property")
    };
    assert_eq!(world.resolve_from(temp, link.form_id), Some(id(1, 0x833)));
    assert!(world.has_keyword(id(0, 0x900), id(1, 0x834)));
    world.restore_vanilla_inventories(&saved).unwrap();
    assert_eq!(world.item_count(id(0, 0x900), id(1, 0x832)), 3);
    let actor = world.actor(id(0, 0x900)).unwrap();
    assert_eq!(actor.values.get("health").base(), 432.0);
    assert_eq!(actor.race, Some(id(0, 0x830)));
    assert_eq!(actor.factions[&id(1, 0x835)], 2);
    assert!(world.actor_base(temp).unwrap().female);
    assert_eq!(world.actor_base(temp).unwrap().class, Some(id(1, 0x830)));
    let mut loaded = World::new(namespace_order());
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(loaded.save_state(), world.save_state());
}

#[test]
fn temporary_base_identity_corruption_and_exhaustion_are_atomic_errors() {
    let saved = fixture();
    let mut world = World::new(fixture_order());
    world.restore_vanilla_reference_placements(&saved).unwrap();
    world.restore_vanilla_actor_templates(&saved).unwrap();
    let temp = world.leveled_actor_base(id(0, 0x900)).unwrap().unwrap();
    let wire =
        |id: GlobalFormId| [id.file.to_le_bytes().as_slice(), &id.object.to_le_bytes()].concat();
    let mut prefix = wire(temp);
    prefix.extend(wire(id(0, 0x810)));
    prefix.extend(wire(id(0, 0x810)));
    prefix.push(1);
    prefix.extend(wire(id(0, 0x900)));
    let bytes = world.save_state();
    let offsets: Vec<_> = bytes
        .windows(prefix.len())
        .enumerate()
        .filter_map(|(at, b)| (b == prefix).then_some(at))
        .collect();
    assert_eq!(offsets.len(), 1);
    let at = offsets[0];
    for (offset, replacement) in [
        (0, wire(id(DYNAMIC_FILE, 0xFFFFFF))),
        (0, wire(id(0, 0x900))),
        (6, wire(id(0, 0x812))),
        (19, wire(id(0, 0x901))),
        (19, wire(id(0, 0x999))),
    ] {
        let mut corrupted = bytes.clone();
        corrupted[at + offset..at + offset + 6].copy_from_slice(&replacement);
        assert!(world.load_state(&corrupted).is_err());
        assert_eq!(world.save_state(), bytes);
    }
    let mut exhausted = saved;
    exhausted
        .table_1
        .iter_mut()
        .find(|b| b.kind == 1)
        .unwrap()
        .data[..4]
        .copy_from_slice(&0xFFFFFFFF_u32.to_le_bytes());
    let mut fresh = World::new(fixture_order());
    fresh
        .restore_vanilla_reference_placements(&exhausted)
        .unwrap();
    let before = fresh.save_state();
    assert!(fresh.restore_vanilla_actor_templates(&exhausted).is_err());
    assert_eq!(fresh.save_state(), before);
}
