// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-03 contact timing through the authored animation interpreter and Game.

#[path = "../../game/tests/common/legacy.rs"]
mod legacy;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{Difficulty, Error, Hand},
    Game,
};
use soe_godot_host::combat::{contact_annotation_triggers, MeleeContact};
use soe_hkx::{
    animation::{Annotation, AnnotationTrack, SplineCompressedAnimation},
    behavior::{BehaviorGraph, BehaviorGraphData, ClipGenerator, Node, NodeId, NodeKind},
    interp::{Clip, ClipLibrary, Interpreter},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn weapon(object: u32, damage: u16, animation: u8, slot: u32) -> Record {
    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = animation;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
        .sub(Tag(*b"ETYP"), slot.to_le_bytes())
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for (slot, all) in [
        (0xb00, false),
        (0xb01, false),
        (0xb02, false),
        (0xb03, true),
    ] {
        let mut record =
            Record::new(Tag(*b"EQUP"), slot).sub(Tag(*b"DATA"), u32::from(all).to_le_bytes());
        if slot >= 0xb02 {
            let mut parents = 0xb00_u32.to_le_bytes().to_vec();
            parents.extend_from_slice(&0xb01_u32.to_le_bytes());
            record = record.sub(Tag(*b"PNAM"), parents);
        }
        plugin.push_top(record);
    }
    plugin.push_top(
        Record::new(Tag(*b"EQUP"), 0xb04)
            .sub(Tag(*b"DATA"), 0_u32.to_le_bytes())
            .sub(Tag(*b"PNAM"), 0xb04_u32.to_le_bytes()),
    );
    let mut defaults = b"LHEQ".to_vec();
    defaults.extend_from_slice(&0xb00_u32.to_le_bytes());
    defaults.extend_from_slice(b"RHEQ");
    defaults.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"DOBJ"), 0x31).sub(Tag(*b"DNAM"), defaults));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for (form, damage, animation, slot) in [
        (0xa00, 10, 1, 0xb02),
        (0xa01, 20, 1, 0xb02),
        (0xa02, 30, 5, 0xb03),
        (0xa03, 10, 1, 0xb03),
        (0xa04, 10, 5, 0xb02),
        (0xa05, 10, 1, 0xb04),
    ] {
        plugin.push_top(weapon(form, damage, animation, slot));
    }
    let mut order = LoadOrder::new();
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0xa06).sub(Tag(*b"CNAM"), 0xa01_u32.to_le_bytes()));
    order.add("Contact.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert_eq!(game.player(), id(0x14));
    {
        let mut world = game.world_mut();
        for actor in [id(0x14), id(0x900)] {
            world.set_actor_value(actor, "health", 100.).unwrap();
        }
        for item in (0xa00..=0xa05).map(id) {
            world.add_item(id(0x14), item, 1);
        }
        world
            .equip_hand_weapon(id(0x14), Hand::Left, id(0xa00))
            .unwrap();
        world
            .equip_hand_weapon(id(0x14), Hand::Right, id(0xa01))
            .unwrap();
    }
    game
}

fn animation() -> (BehaviorGraph, ClipLibrary) {
    let source = SplineCompressedAnimation {
        duration: 1.0,
        num_transform_tracks: 0,
        num_float_tracks: 0,
        annotation_tracks: vec![AnnotationTrack {
            track_name: "weapon".into(),
            annotations: vec![
                Annotation {
                    time: 0.1,
                    text: "weaponSwing".into(),
                },
                Annotation {
                    time: 0.2,
                    text: "preHitFrame".into(),
                },
                Annotation {
                    time: 0.5,
                    text: "HitFrame".into(),
                },
                Annotation {
                    time: 0.9,
                    text: "attackStop".into(),
                },
            ],
        }],
        num_frames: 0,
        num_blocks: 0,
        max_frames_per_block: 0,
        mask_and_quantization_size: 0,
        block_duration: 1.0,
        block_inverse_duration: 1.0,
        frame_duration: 0.0,
        block_offsets: vec![],
        float_block_offsets: vec![],
        transform_offsets: vec![],
        float_offsets: vec![],
        data: vec![],
        endian: 0,
        extracted_motion: None,
    };
    let mut clips = ClipLibrary::new();
    clips.insert(Clip::from_parts("swing", source, &[]).unwrap());
    let mut graph = BehaviorGraph {
        name: "contact".into(),
        variable_mode: 0,
        root: Some(NodeId(0)),
        data: BehaviorGraphData::default(),
        nodes: vec![Node {
            id: NodeId(0),
            offset: 0,
            class: "hkbClipGenerator".into(),
            name: "swing".into(),
            user_data: 0,
            bindings: None,
            kind: NodeKind::Clip(ClipGenerator {
                animation_name: "swing".into(),
                playback_speed: 1.0,
                ..ClipGenerator::default()
            }),
        }],
    };
    contact_annotation_triggers(&mut graph, &clips);
    (graph, clips)
}

#[test]
fn cmb_03_authored_contact_frame_applies_one_hit_with_the_selected_hand() {
    for (hand, damage) in [(Hand::Left, 10.0), (Hand::Right, 20.0)] {
        let mut game = fixture();
        let mut contact = MeleeContact::default();
        contact.begin(&game, id(0x14), hand).unwrap();
        let (graph, clips) = animation();
        let mut vm = Interpreter::new(&graph, &clips, vec![]);
        let mut queries = 0;
        for (delta, expected) in [
            (0.25, 100.0),
            (0.24, 100.0),
            (0.01, 100.0 - damage),
            (0.5, 100.0 - damage),
        ] {
            vm.update(delta);
            for event in vm.take_events() {
                contact
                    .event(
                        &mut game,
                        &graph.data.event_names[event.id as usize],
                        |_| {
                            queries += 1;
                            Ok(Some(id(0x900)))
                        },
                    )
                    .unwrap();
            }
            assert_eq!(game.world_mut().actor_value(id(0x900), "health"), expected);
        }
        contact
            .event(&mut game, "HitFrame", |_| {
                queries += 1;
                Ok(Some(id(0x900)))
            })
            .unwrap();
        assert_eq!(queries, 1);
    }
}

#[test]
fn misses_and_cancellation_consume_contact_without_late_damage() {
    let mut game = fixture();
    let mut contact = MeleeContact::default();
    contact.begin(&game, id(0x14), Hand::Right).unwrap();
    assert!(contact.begin(&game, id(0x14), Hand::Left).is_err());
    assert!(contact
        .event(&mut game, "HitFrame", |_| Ok(None))
        .unwrap()
        .is_none());
    assert!(contact
        .event(&mut game, "HitFrame", |_| panic!("duplicate query"))
        .unwrap()
        .is_none());
    contact.begin(&game, id(0x14), Hand::Left).unwrap();
    contact
        .event(&mut game, "attackStop", |_| panic!("cancel query"))
        .unwrap();
    assert!(contact
        .event(&mut game, "HitFrame", |_| panic!("cancelled hit"))
        .unwrap()
        .is_none());
    assert_eq!(game.world_mut().actor_value(id(0x900), "health"), 100.0);
}

#[test]
fn equipment_changes_and_rejected_contacts_cannot_retry() {
    let mut game = fixture();
    let mut contact = MeleeContact::default();
    contact.begin(&game, id(0x14), Hand::Right).unwrap();
    game.world_mut()
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa02))
        .unwrap();
    assert!(matches!(
        contact.event(&mut game, "HitFrame", |_| panic!("stale weapon")),
        Err(Error::Equipment)
    ));
    assert!(contact
        .event(&mut game, "HitFrame", |_| panic!("retry"))
        .unwrap()
        .is_none());
    contact.begin(&game, id(0x14), Hand::Right).unwrap();
    // Essential targets now enter bleedout (CMB-13), so a target already down
    // supplies the rejected hit.
    game.world_mut().actor(id(0x900)).unwrap().unconscious = true;
    assert!(contact
        .event(&mut game, "HitFrame", |_| Ok(Some(id(0x900))))
        .is_err());
    assert!(contact
        .event(&mut game, "HitFrame", |_| panic!("retry rejected damage"))
        .unwrap()
        .is_none());
    assert_eq!(game.world_mut().actor_value(id(0x900), "health"), 100.0);
}

#[test]
fn two_handed_selection_clears_left_and_requires_distinct_inventory_copies() {
    let game = fixture();
    let player = id(0x14);
    let mut world = game.world_mut();
    assert!(world
        .equip_hand_weapon(player, Hand::Right, id(0xa00))
        .is_err());
    world.add_item(player, id(0xa00), 1);
    world
        .equip_hand_weapon(player, Hand::Right, id(0xa00))
        .unwrap();
    assert_eq!(world.hand_weapon(player, Hand::Left), Some(id(0xa00)));
    world
        .equip_hand_weapon(player, Hand::Right, id(0xa02))
        .unwrap();
    assert_eq!(world.hand_weapon(player, Hand::Left), None);
    assert!(!world
        .actor_ref(player)
        .unwrap()
        .equipped
        .contains(&id(0xa00)));
    assert!(world
        .equip_hand_weapon(player, Hand::Left, id(0xa00))
        .is_err());
}

#[test]
fn annotation_conversion_is_idempotent_and_honors_disable_flag() {
    let (mut graph, clips) = animation();
    let before = graph.clone();
    contact_annotation_triggers(&mut graph, &clips);
    assert_eq!(graph, before);
    if let NodeKind::Clip(clip) = &mut graph.nodes[0].kind {
        clip.triggers.clear();
        clip.flags = 16;
    }
    contact_annotation_triggers(&mut graph, &clips);
    let NodeKind::Clip(clip) = &graph.nodes[0].kind else {
        panic!("fixture")
    };
    assert!(clip.triggers.is_empty());
}

#[test]
fn hands_survive_save_load_and_change_the_world_hash() {
    let mut game = fixture();
    let before = game.world().state_hash();
    let save = game.save_state().unwrap();
    game.world_mut()
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa02))
        .unwrap();
    assert_ne!(game.world().state_hash(), before);
    game.load_state(&save).unwrap();
    assert_eq!(
        game.world_mut().hand_weapon(id(0x14), Hand::Left),
        Some(id(0xa00))
    );
    assert_eq!(
        game.world_mut().hand_weapon(id(0x14), Hand::Right),
        Some(id(0xa01))
    );
    assert_eq!(game.world().state_hash(), before);
}

#[test]
fn equipment_slots_control_hands_independently_of_animation_type() {
    let game = fixture();
    let mut world = game.world_mut();
    world
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa03))
        .unwrap();
    assert_eq!(world.hand_weapon(id(0x14), Hand::Left), None);
    world
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa01))
        .unwrap();
    world
        .equip_hand_weapon(id(0x14), Hand::Left, id(0xa04))
        .unwrap();
    assert_eq!(world.hand_weapon(id(0x14), Hand::Left), Some(id(0xa04)));
}

#[test]
fn cyclic_equipment_parent_rejects_before_changing_equipment() {
    let game = fixture();
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.world_mut()
            .equip_hand_weapon(id(0x14), Hand::Right, id(0xa05)),
        Err(Error::Unsupported("cyclic equipment slot parents"))
    ));
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn removed_hand_weapon_stays_unusable_after_loading() {
    let mut game = fixture();
    game.world_mut()
        .actor(id(0x14))
        .unwrap()
        .equipped
        .remove(&id(0xa01));
    assert_eq!(game.world_mut().hand_weapon(id(0x14), Hand::Right), None);
    let save = game.save_state().unwrap();
    game.load_state(&save).unwrap();
    let mut contact = MeleeContact::default();
    assert!(matches!(
        contact.begin(&game, id(0x14), Hand::Right),
        Err(Error::Equipment)
    ));
}

#[test]
fn templated_hand_weapon_reaches_contact_damage_with_selected_identity() {
    let mut game = fixture();
    {
        let mut world = game.world_mut();
        world.add_item(id(0x14), id(0xa06), 1);
        world
            .equip_hand_weapon(id(0x14), Hand::Right, id(0xa06))
            .unwrap();
        assert_eq!(world.hand_weapon(id(0x14), Hand::Right), Some(id(0xa06)));
    }
    let mut contact = MeleeContact::default();
    contact.begin(&game, id(0x14), Hand::Right).unwrap();
    let hit = contact
        .event(&mut game, "HitFrame", |_| Ok(Some(id(0x900))))
        .unwrap()
        .unwrap();
    assert_eq!(hit.health_damage, 20.);
}

#[test]
fn combined_save_reads_both_branch_layouts_without_confusing_their_extensions() {
    let game = fixture();
    game.world_mut().set_difficulty(Difficulty::Legendary);
    let actors = game.world().loaded_actors();
    let bytes = legacy::without_exclusive_branches(&mut game.world_mut(), &actors);
    // Independently construct the one-player hand table in this fixture.
    let mut hands = 1_u32.to_le_bytes().to_vec();
    hands.extend_from_slice(&0_u16.to_le_bytes());
    hands.extend_from_slice(&0x14_u32.to_le_bytes());
    for weapon in [0xa00_u32, 0xa01] {
        hands.push(1);
        hands.extend_from_slice(&0_u16.to_le_bytes());
        hands.extend_from_slice(&weapon.to_le_bytes());
    }
    let mut suffix = vec![Difficulty::Legendary as u8];
    suffix.extend_from_slice(&hands);
    // The empty version 20 reference extras, then the empty version 21 cursors,
    // then the empty version 25 image-space block: two counts and a fade flag.
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.push(0);
    // Empty loose items, zone levels, zone visits and repeat history.
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    suffix.extend_from_slice(&0_u32.to_le_bytes());
    assert!(bytes.ends_with(&suffix));
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        soe_game::world::SAVE_VERSION
    );
    for version in [15_u16, 16, 17, 18] {
        let mut migrated = bytes[..bytes.len() - suffix.len()].to_vec();
        migrated[4..6].copy_from_slice(&version.to_le_bytes());
        if version == 16 || version == 18 {
            migrated.push(Difficulty::Legendary as u8);
        }
        if version >= 17 {
            migrated.extend_from_slice(&hands);
        }
        let restored = fixture();
        let mut world = restored.world_mut();
        world.load_state(&migrated).unwrap();
        assert_eq!(
            world.difficulty(),
            if version == 16 || version == 18 {
                Difficulty::Legendary
            } else {
                Difficulty::Adept
            }
        );
        assert_eq!(
            world.hand_weapon(id(0x14), Hand::Right),
            (version >= 17).then_some(id(0xa01))
        );
        if version == 18 {
            assert_eq!(world.state_hash(), game.world().state_hash());
        }
    }
    let before = game.world().state_hash();
    let mut invalid = bytes.clone();
    invalid[bytes.len() - suffix.len()] = 255;
    assert!(game.world_mut().load_state(&invalid).is_err());
    assert_eq!(game.world().state_hash(), before);
}
