// SPDX-License-Identifier: GPL-3.0-or-later

//! Created identity separation and typed migration of the legacy SOEW namespace.

use super::*;
use crate::inventory::{Extra, Field, Stack};
use crate::quest::state::{AliasMetadata, JournalInstance, RunData, StoryEvent, StoryParameter};
use crate::world::magic::{ActiveEffect, EffectId};
use crate::world::record_overrides::{raw_words, RecordOverride};
use crate::world::templates::TemplateChoice;
use crate::world::DYNAMIC_FILE;
use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Slot, Tag};

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn full_order(regular_at_255: bool) -> LoadOrder {
    let mut order = LoadOrder::new();
    let prefix = if regular_at_255 { 2 } else { 0 };
    for (slot, light) in (0..prefix)
        .map(|n| (n, true))
        .chain((0..254).map(|n| (n, false)))
        .chain((prefix..4096).map(|n| (n, true)))
    {
        let mut plugin = Plugin::new(FileHeader {
            flags: if light { flags::LIGHT_MASTER } else { 0 },
            ..FileHeader::default()
        });
        plugin.push_top(Record::new(Tag(*b"MISC"), 0x801));
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 0x802).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()),
        );
        order
            .add(&format!("Plugin{slot}-{light}.esp"), plugin)
            .unwrap();
    }
    order
}

#[test]
fn sav_07_full_load_order_and_created_references_round_trip_without_aliases() {
    for regular_at_255 in [false, true] {
        let order = std::sync::Arc::new(full_order(regular_at_255));
        assert_eq!(order.files().len(), 4350);
        assert_eq!(
            matches!(order.file(255).slot, Slot::Regular(_)),
            regular_at_255
        );
        let mut world = World::from_shared_order(order.clone());
        world.next_dynamic = 0x800;
        let created = world.place_at(id(255, 0x801), id(255, 0x802), false);
        assert_eq!(created, id(DYNAMIC_FILE, 0x801));
        let mut runtime_ids = BTreeSet::new();
        for (form, _) in order.winners() {
            let raw = world.runtime_id(form);
            assert!(runtime_ids.insert(raw));
            assert_eq!(world.from_runtime_id(raw), Some(form));
        }
        assert!(runtime_ids.insert(world.runtime_id(created)));
        assert_eq!(order.runtime_id(created), 0xFF00_0801);
        assert_eq!(world.from_runtime_id(0xFF00_0801), Some(created));
        assert_eq!(world.from_runtime_id(0xFF00_0802), None);
        assert_eq!(
            world.reference_ref(created).unwrap().base,
            Some(id(255, 0x801))
        );
        let mut loaded = World::from_shared_order(order);
        loaded.load_state(&world.save_state()).unwrap();
        assert_eq!(loaded.save_state(), world.save_state());
        assert_eq!(loaded.from_runtime_id(0xFF00_0801), Some(created));
        assert_eq!(
            loaded.place_at(id(255, 0x801), created, false),
            id(DYNAMIC_FILE, 0x802)
        );
        assert!(loaded.exists(id(255, 0x802)));
    }
}

fn legacy_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(crate::npc::build_record(
        0x820,
        "Base",
        "Base",
        crate::npc::Config::default(),
        crate::npc::Stats::default(),
    ));
    plugin.push_top(crate::quest::with_alias(
        crate::quest::build_record(0x910, "Quest", 0),
        5,
        "Target",
        None,
    ));
    plugin.push_top(Record::new(Tag(*b"MGEF"), 0x850).sub(Tag(*b"DATA"), vec![0; 152]));
    plugin.push_top(
        Record::new(Tag(*b"SPEL"), 0x851)
            .sub(Tag(*b"SPIT"), vec![0; 36])
            .sub(Tag(*b"EFID"), 0x850_u32.to_le_bytes())
            .sub(Tag(*b"EFIT"), vec![0; 12]),
    );
    let mut order = LoadOrder::new();
    order.add("Legacy.esm", plugin).unwrap();
    order
}

fn rich_world(namespace: u16) -> World {
    let mut world = World::new(legacy_order());
    world.difficulty = Difficulty::Legendary;
    let actor = id(namespace, 0x800);
    let other = id(namespace, 0x801);
    let temporary = id(namespace, 0x802);
    let quest = id(0, 0x910);
    let origin = id(0, 0x820);
    let alias = Handle::Alias { quest, id: 5 };
    for (index, handle) in [
        Handle::Form(actor),
        Handle::Form(other),
        alias,
        Handle::ActiveEffect(EffectId(1)),
    ]
    .into_iter()
    .enumerate()
    {
        let object = ObjectId(index as u64 + 1);
        world.objects.insert(handle, object);
        world.handles.insert(object, handle);
    }
    let mut reference = RefState::new(Some(origin));
    reference.is_actor = true;
    reference.cell = Some(other);
    reference.owner = Some(other);
    reference.location = Some(other);
    reference.linked = vec![(Some(temporary), other)];
    reference.teleport = Some((
        other,
        Transform {
            position: [1.0; 3],
            rotation: [2.0; 3],
        },
    ));
    reference
        .inventory
        .restore(
            temporary,
            1,
            vec![Stack::new(
                1,
                vec![Extra {
                    kind: 153,
                    fields: vec![Field::Array(vec![vec![Field::Group(vec![
                        Field::Form(Some(other)),
                        Field::Short(255),
                        Field::Word(255),
                        Field::Bytes(vec![255, 0, 1, 8, 0, 0]),
                    ])]])],
                }],
            )
            .unwrap()],
        )
        .unwrap();
    world.refs.insert(actor, reference.clone());
    world.refs.insert(other, reference);
    let mut actor_state = ActorState {
        combat_target: Some(other),
        killer: Some(other),
        ash_pile: Some(other),
        deferred_kill: true,
        left_weapon: Some(temporary),
        right_weapon: Some(other),
        ..ActorState::default()
    };
    actor_state.ai.package = Some(temporary);
    actor_state.ai.alias = Some((other, 5));
    actor_state.ai.bed = Some(other);
    actor_state.ai.center_cell = Some(other);
    actor_state.ai.path = vec![[1.0; 3]];
    actor_state.ai.path_cells = vec![Some(other)];
    actor_state.ai.completed.insert(temporary, 7);
    actor_state.relationships.insert(other, 3);
    actor_state.equipped.insert(temporary);
    world.actors.insert(actor, actor_state);
    world.actors.insert(other, ActorState::default());

    let mut state = world.initial_quest(quest).unwrap();
    state.aliases.insert(5, Some(actor));
    state.journal.instances.insert(
        0,
        JournalInstance {
            aliases: BTreeMap::from([(5, Some(other))]),
            globals: BTreeMap::from([(temporary, 42.0)]),
            ..JournalInstance::default()
        },
    );
    state.runtime.run_data = Some(RunData {
        aliases: BTreeMap::from([(
            5,
            AliasMetadata {
                kind: 1,
                references: vec![Some(actor), Some(other), None, Some(temporary)],
            },
        )]),
        event_flag: 1,
        event: Some(StoryEvent {
            id: 7,
            code: *b"SCPT",
            parameters: vec![StoryParameter::Reference {
                kind: 0,
                reference: Some(other),
            }],
        }),
        ..RunData::default()
    });
    world.quests.insert(quest, state);
    world
        .form_lists
        .insert(quest, vec![actor, other, temporary]);
    world.player = Some(actor);
    world.loaded_cell = Some(other);
    world.pending.push(Pending::DeferredDeath {
        actor,
        killer: Some(other),
        silent: false,
    });
    world.pending.push(Pending::ActorDeath {
        actor: other,
        killer: Some(actor),
        recipients: vec![Handle::Form(other), alias],
    });
    world.animation_events.push((actor, "Animation".into()));
    world.last_ridden_horse = Some(other);
    world.sound_instances.insert(1, (temporary, actor, 1.0));
    world.spell_casts.push((id(0, 0x851), actor, Some(other)));
    world.dialogue.session = Some(crate::dialogue::Session {
        speaker: actor,
        listener: other,
        topic: None,
        current_info: None,
        last_info: None,
        response_index: 0,
    });
    world.dialogue.said.insert(temporary, 1.0);
    world.next_effect = 2;
    world.active_effects.insert(
        EffectId(1),
        ActiveEffect {
            id: EffectId(1),
            spell: id(0, 0x851),
            effect: id(0, 0x850),
            effect_index: 0,
            source: actor,
            caster: Some(actor),
            target: other,
            magnitude: 1.0,
            duration: Some(60.0),
            elapsed: 1.0,
            summoned: None,
            recover: false,
            changes: vec![],
        },
    );
    world.actor_templates.insert(
        actor,
        TemplateChoice {
            temporary_base: Some(temporary),
            level: None,
            chain: Some(vec![origin]),
        },
    );
    let mut record = world.order.winner(origin).unwrap().clone();
    record.form_id = 0xFF00_0000 | temporary.object;
    let mut value = RecordOverride::new(origin, record);
    value.temporary_for = Some(actor);
    value.npc.original_race = Some(other);
    let raw = value
        .link(&mut raw_words(&value.record), Some(temporary))
        .unwrap();
    value.set(Tag(*b"ZZZZ"), raw.to_le_bytes());
    world.record_overrides.insert(temporary, value);
    world
}

/// Save `world` the way a version 23 engine would, without the exclusive
/// dialogue branch field that version 24 added, the stagger end time field
/// that version 29 added, the witnessed crimes set that version 31 added,
/// the hand spell fields that version 32 added, and the summon actor link
/// that version 33 added, plus the record metadata byte from version 35.
/// Each is set to a unique sentinel so its serialized
/// bytes can be located exactly, then cut back out.
fn without_exclusive_branches(world: &mut World) -> Vec<u8> {
    let ids: Vec<_> = world.actors.keys().copied().collect();
    let mut markers = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xF000_0000 + index as u32,
        };
        world.actors.get_mut(id).unwrap().exclusive_branch = Some(sentinel);
        let mut marker = vec![1];
        marker.extend_from_slice(&sentinel.file.to_le_bytes());
        marker.extend_from_slice(&sentinel.object.to_le_bytes());
        markers.push(marker);

        let stagger_sentinel = 1_000_000.0 + f64::from(index as u32);
        world.actors.get_mut(id).unwrap().stagger_end_time = Some(stagger_sentinel);
        let mut stagger_marker = vec![1];
        stagger_marker.extend_from_slice(&stagger_sentinel.to_le_bytes());
        markers.push(stagger_marker);

        // `witnessed_crimes` (version 31) is written unconditionally, even
        // when empty, so an emulated pre-30 save must strip it same as the
        // two optional fields above. A one-element set gives a marker as
        // locatable as `exclusive_branch`'s.
        let crime_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xD000_0000 + index as u32,
        };
        world.actors.get_mut(id).unwrap().witnessed_crimes = BTreeSet::from([crime_sentinel]);
        let mut crime_marker = 1_u32.to_le_bytes().to_vec();
        crime_marker.extend_from_slice(&crime_sentinel.file.to_le_bytes());
        crime_marker.extend_from_slice(&crime_sentinel.object.to_le_bytes());
        markers.push(crime_marker);

        // `left_spell` and `right_spell` (version 32) are likewise written
        // unconditionally, so a pre-32 emulated save must strip both.
        let left_spell_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xE000_0000 + index as u32,
        };
        let right_spell_sentinel = GlobalFormId {
            file: u16::MAX,
            object: 0xE100_0000 + index as u32,
        };
        world.actors.get_mut(id).unwrap().left_spell = Some(left_spell_sentinel);
        world.actors.get_mut(id).unwrap().right_spell = Some(right_spell_sentinel);
        let mut left_spell_marker = vec![1];
        left_spell_marker.extend_from_slice(&left_spell_sentinel.file.to_le_bytes());
        left_spell_marker.extend_from_slice(&left_spell_sentinel.object.to_le_bytes());
        markers.push(left_spell_marker);
        let mut right_spell_marker = vec![1];
        right_spell_marker.extend_from_slice(&right_spell_sentinel.file.to_le_bytes());
        right_spell_marker.extend_from_slice(&right_spell_sentinel.object.to_le_bytes());
        markers.push(right_spell_marker);
    }
    let summon_sentinel = GlobalFormId {
        file: u16::MAX,
        object: 0xE200_0000,
    };
    world.active_effects.get_mut(&EffectId(1)).unwrap().summoned = Some(summon_sentinel);
    let mut summon_marker = vec![1];
    summon_marker.extend_from_slice(&summon_sentinel.file.to_le_bytes());
    summon_marker.extend_from_slice(&summon_sentinel.object.to_le_bytes());
    markers.push(summon_marker);
    let mut bytes = world.save_state();
    world.active_effects.get_mut(&EffectId(1)).unwrap().summoned = None;
    for id in &ids {
        let actor = world.actors.get_mut(id).unwrap();
        actor.exclusive_branch = None;
        actor.stagger_end_time = None;
        actor.witnessed_crimes.clear();
        actor.left_spell = None;
        actor.right_spell = None;
    }
    for marker in markers {
        let at = bytes
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("sentinel field is serialized");
        bytes.drain(at..at + marker.len());
    }
    let mut records = Writer::new();
    world.write_record_overrides(&mut records);
    let at = bytes
        .windows(records.out.len())
        .position(|window| window == records.out)
        .expect("record override is serialized");
    // CONTEXT: This fixture has one override with no list fields in version 23.
    bytes.remove(at + 39);
    bytes
}

#[test]
fn sav_07_legacy_ids_migrate_through_all_nested_world_state() {
    for version in 15..=18_u16 {
        let mut legacy = without_exclusive_branches(&mut rich_world(255));
        // Drop empty post-version-19 blocks, including zone visit and scene history.
        legacy.truncate(legacy.len() - 37);
        let tail = legacy.len() - 25;
        let mut expected = rich_world(DYNAMIC_FILE);
        match version {
            15 => legacy.truncate(tail),
            16 => legacy.truncate(tail + 1),
            17 => {
                legacy.remove(tail);
            }
            _ => {}
        }
        if version < 17 {
            for actor in expected.actors.values_mut() {
                actor.left_weapon = None;
                actor.right_weapon = None;
            }
        }
        if version == 15 || version == 17 {
            expected.difficulty = Difficulty::Adept;
        }
        legacy[4..6].copy_from_slice(&version.to_le_bytes());
        let mut loaded = World::new(legacy_order());
        loaded.load_state(&legacy).unwrap();
        assert_eq!(
            loaded.save_state(),
            expected.save_state(),
            "version {version}"
        );
        assert_eq!(loaded.state_hash(), expected.state_hash());
        let current = loaded.save_state();
        loaded.load_state(&current).unwrap();
        assert_eq!(loaded.save_state(), current);
    }
}

#[test]
fn sav_16_extras_preserve_created_namespace_and_all_existing_world_state() {
    let mut world = rich_world(DYNAMIC_FILE);
    let values = &mut world
        .actors
        .get_mut(&id(DYNAMIC_FILE, 0x800))
        .unwrap()
        .values;
    values.set_base("health", 100.0).unwrap();
    values.modify("health", 12.0).unwrap();
    values.modify_temporary("health", 23.0).unwrap();
    values.damage("health", 34.0).unwrap();
    let reference = world.refs.get_mut(&id(DYNAMIC_FILE, 0x800)).unwrap();
    reference.owner_rank = Some(-7);
    reference.lock_level = 75;
    reference.locked = true;
    reference.lock_extra = Some(super::super::ReferenceLock {
        key: None,
        flags: 4,
        unknown: [0x1234_5678, 0xAABB_CCDD],
    });
    let before = world.save_state();
    let mut loaded = World::new(legacy_order());
    loaded.load_state(&before).unwrap();
    assert_eq!(loaded.save_state(), before);
    assert_eq!(loaded.state_hash(), world.state_hash());

    let mut legacy = without_exclusive_branches(&mut rich_world(DYNAMIC_FILE));
    legacy.truncate(legacy.len() - 37);
    legacy[4..6].copy_from_slice(&19_u16.to_le_bytes());
    loaded.load_state(&legacy).unwrap();
    assert_eq!(loaded.save_state(), rich_world(DYNAMIC_FILE).save_state());
}

#[test]
fn sav_07_ambiguous_legacy_loadouts_fail_before_world_or_vm_mutation() {
    let mut game = crate::Game::new(full_order(true));
    game.world_mut()
        .notifications
        .push("keep existing state".into());
    let before = game.save_state().unwrap();
    for version in 4..=18_u16 {
        let mut bytes = before.clone();
        bytes[14..16].copy_from_slice(&version.to_le_bytes());
        assert!(game
            .load_state(&bytes)
            .unwrap_err()
            .to_string()
            .contains("ambiguous created identities"));
        assert_eq!(game.save_state().unwrap(), before);
    }
}

#[test]
fn sav_07_unknown_version_and_mixed_legacy_namespaces_are_rejected() {
    let mut world = rich_world(DYNAMIC_FILE);
    let before = world.save_state();
    for (version, message) in [
        (37_u16, "world state version 37"),
        (18, "created namespace in a legacy world state"),
    ] {
        let mut bytes = before.clone();
        bytes[4..6].copy_from_slice(&version.to_le_bytes());
        assert!(world
            .load_state(&bytes)
            .unwrap_err()
            .to_string()
            .contains(message));
        assert_eq!(world.save_state(), before);
    }
}

#[test]
fn sav_07_legacy_migration_boundary_is_the_first_colliding_plugin_position() {
    let mut order = legacy_order();
    for slot in 0..254 {
        order
            .add(
                &format!("Light{slot}.esl"),
                Plugin::new(FileHeader {
                    flags: flags::LIGHT_MASTER,
                    ..FileHeader::default()
                }),
            )
            .unwrap();
    }
    let mut legacy = without_exclusive_branches(&mut rich_world(255));
    legacy.truncate(legacy.len() - 37);
    legacy[4..6].copy_from_slice(&18_u16.to_le_bytes());
    let mut world = World::new(order);
    world.load_state(&legacy).unwrap();
    assert!(world.exists(id(DYNAMIC_FILE, 0x800)));
    let mut order = std::sync::Arc::try_unwrap(world.order).ok().unwrap();
    order
        .add("Position255.esp", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut world = World::new(order);
    let before = world.save_state();
    assert!(world
        .load_state(&legacy)
        .unwrap_err()
        .to_string()
        .contains("ambiguous created identities"));
    assert_eq!(world.save_state(), before);
}

#[test]
fn an_actor_state_before_version_24_defaults_an_absent_exclusive_branch() {
    let mut actor = ActorState {
        exclusive_branch: Some(GlobalFormId {
            file: 0,
            object: 0x1903,
        }),
        ..ActorState::default()
    };
    actor.relationships.insert(
        GlobalFormId {
            file: 2,
            object: 0x1234,
        },
        -3,
    );
    actor.combat_target = Some(GlobalFormId {
        file: 3,
        object: 0x5678,
    });
    actor.graph_floats.insert("speed".into(), 1.5);
    let mut writer = Writer::new();
    write_actor_state(&mut writer, &actor);
    let marker = [1, 0, 0, 3, 0x19, 0, 0];
    let offset = writer
        .out
        .windows(marker.len())
        .position(|bytes| bytes == marker)
        .expect("exclusive branch is serialized");
    writer.out.drain(offset..offset + marker.len());
    actor.exclusive_branch = None;

    let mut reader = Reader::new(&writer.out);
    let mut reader = FormReader::new(&mut reader, FormNamespace::Current);
    let restored = read_actor_state(&mut reader, 23).unwrap();
    assert_eq!(restored, actor);
}
