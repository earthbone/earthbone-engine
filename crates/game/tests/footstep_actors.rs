// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::common::installed;

use std::collections::BTreeSet;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::footsteps::Footsteps;
use soe_game::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn npc(object: u32, female: bool) -> Record {
    let mut acbs = vec![0; 24];
    acbs[0] = u8::from(female);
    Record::new(T(b"NPC_"), object)
        .sub(T(b"ACBS"), acbs)
        .sub(T(b"RNAM"), 0x100_u32.to_le_bytes())
}

fn addon(object: u32, slots: u32, priority: [u8; 2], set: u32) -> Record {
    let mut data = vec![0; 12];
    data[..2].copy_from_slice(&priority);
    Record::new(T(b"ARMA"), object)
        .sub(
            T(b"BOD2"),
            [slots.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
        )
        .sub(T(b"DNAM"), data)
        .sub(T(b"RNAM"), 0x100_u32.to_le_bytes())
        .sub(T(b"SNDD"), set.to_le_bytes())
}

fn armor(object: u32, addon: u32) -> Record {
    Record::new(T(b"ARMO"), object).sub(T(b"MODL"), addon.to_le_bytes())
}

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in [
        Record::new(T(b"DOBJ"), 0x31).sub(
            T(b"DNAM"),
            [b"DFTS".as_slice(), &0x900_u32.to_le_bytes()].concat(),
        ),
        npc(0x10, false),
        npc(0x11, true),
        Record::new(T(b"RACE"), 0x100).sub(T(b"WNAM"), 0x200_u32.to_le_bytes()),
        Record::new(T(b"RACE"), 0x101).sub(T(b"RNAM"), 0x100_u32.to_le_bytes()),
        Record::new(T(b"RACE"), 0x102),
        armor(0x200, 0x300),
        armor(0x201, 0x301),
        armor(0x202, 0x302),
        addon(0x300, 1 << 7, [0, 0], 0x901),
        addon(0x301, (1 << 7) | (1 << 8), [10, 5], 0x902),
        addon(0x302, 1 << 7, [5, 10], 0x903),
        Record::new(T(b"WEAP"), 0x400),
    ] {
        plugin.push_top(record);
    }
    for set in 0x900..=0x903 {
        plugin.push_top(Record::new(T(b"FSTS"), set));
    }
    plugin
}

fn order() -> LoadOrder {
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin()).unwrap();
    order
}

#[test]
fn implicit_player_uses_the_player_base_for_race_and_skin() {
    let mut plugin = plugin();
    plugin.push_top(npc(7, false).zsub(T(b"EDID"), "Player"));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut world = World::new(order);
    let player = world.player();
    assert_eq!(player, id(0x14));
    assert_eq!(world.base_of(player), Some(id(7)));
    assert_eq!(world.footstep_sets(player).unwrap(), vec![id(0x901)]);
}

#[test]
fn world_equipment_and_race_changes_take_effect_on_the_next_event() {
    let mut world = World::new(order());
    let actor = world.place_at(id(0x10), id(0x14), false);
    assert_eq!(world.footstep_sets(actor).unwrap(), vec![id(0x901)]);
    world
        .actor(actor)
        .unwrap()
        .equipped
        .extend([id(0x201), id(0x400)]);
    assert_eq!(world.footstep_sets(actor).unwrap(), vec![id(0x902)]);
    world.actor(actor).unwrap().equipped.clear();
    assert_eq!(world.footstep_sets(actor).unwrap(), vec![id(0x901)]);
    world.actor(actor).unwrap().race = Some(id(0x102));
    assert_eq!(world.footstep_sets(actor).unwrap(), vec![id(0x900)]);
}

#[test]
fn sex_priorities_apply_per_slot_and_surviving_sets_are_not_duplicated() {
    let order = order();
    let steps = Footsteps::new(&order);
    let equipped = BTreeSet::from([id(0x201), id(0x202)]);
    assert_eq!(
        steps.actor_sets(Some(id(0x10)), None, &equipped).unwrap(),
        vec![id(0x902)]
    );
    // The female boot wins feet, while the other addon still occupies calves.
    assert_eq!(
        steps.actor_sets(Some(id(0x11)), None, &equipped).unwrap(),
        vec![id(0x902), id(0x903)]
    );
}

#[test]
fn templated_armor_uses_inherited_armatures_in_the_defining_namespace() {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    patch.push_top(
        Record::new(T(b"ARMO"), 0x0100_0800)
            .zsub(T(b"FULL"), "Child boots")
            .sub(T(b"TNAM"), 0x201_u32.to_le_bytes()),
    );
    order.add("Child.esp", patch).unwrap();
    let child = GlobalFormId {
        file: 1,
        object: 0x800,
    };
    assert_eq!(
        Footsteps::new(&order)
            .actor_sets(None, Some(id(0x100)), &BTreeSet::from([child]))
            .unwrap(),
        vec![id(0x902)]
    );
}

#[test]
fn additional_races_and_armor_parent_chains_preserve_override_namespaces() {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    let mut replacement = addon(0x301, 1 << 7, [10, 10], 0x0100_0900);
    replacement.subrecords.retain(|s| s.tag != T(b"RNAM"));
    replacement = replacement
        .sub(T(b"RNAM"), 0x102_u32.to_le_bytes())
        .sub(T(b"MODL"), 0x100_u32.to_le_bytes());
    patch.push_top(replacement);
    patch.push_top(Record::new(T(b"FSTS"), 0x0100_0900));
    order.add("Sound.esp", patch).unwrap();
    let sets = Footsteps::new(&order)
        .actor_sets(None, Some(id(0x101)), &BTreeSet::from([id(0x201)]))
        .unwrap();
    assert_eq!(
        sets,
        vec![GlobalFormId {
            file: 1,
            object: 0x900
        }]
    );
}

#[test]
fn silent_addons_cover_skin_and_npc_skin_precedes_race_skin() {
    let mut plugin = plugin();
    plugin.push_top(armor(0x203, 0x303));
    plugin.push_top(addon(0x303, 1 << 7, [10, 10], 0));
    plugin.push_top(npc(0x12, false).sub(T(b"WNAM"), 0x201_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let steps = Footsteps::new(&order);
    assert_eq!(
        steps
            .actor_sets(Some(id(0x12)), None, &BTreeSet::new())
            .unwrap(),
        vec![id(0x902)]
    );
    assert_eq!(
        steps
            .actor_sets(Some(id(0x10)), None, &BTreeSet::from([id(0x203)]))
            .unwrap(),
        vec![id(0x900)]
    );
}

#[test]
fn trait_templates_resolve_links_in_the_template_owner_and_reject_cycles() {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    let mut acbs = vec![0; 24];
    acbs[18] = 1;
    patch.push_top(
        Record::new(T(b"NPC_"), 0x10)
            .sub(T(b"ACBS"), acbs)
            .sub(T(b"TPLT"), 0x0100_0010_u32.to_le_bytes()),
    );
    patch.push_top(npc(0x0100_0010, false).sub(T(b"WNAM"), 0x201_u32.to_le_bytes()));
    order.add("Traits.esp", patch).unwrap();
    assert_eq!(
        Footsteps::new(&order)
            .actor_sets(Some(id(0x10)), None, &BTreeSet::new())
            .unwrap(),
        vec![id(0x902)]
    );
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    patch.push_top(Record::new(T(b"RACE"), 0x100).sub(T(b"RNAM"), 0x101_u32.to_le_bytes()));
    order.add("Cycle.esp", patch).unwrap();
    assert!(Footsteps::new(&order)
        .actor_sets(Some(id(0x10)), None, &BTreeSet::new())
        .unwrap_err()
        .to_string()
        .contains("cycle"));
}

#[test]
fn malformed_addon_fields_and_missing_sets_are_errors() {
    for record in [
        Record::new(T(b"ARMA"), 0x301).sub(T(b"RNAM"), vec![0; 2]),
        addon(0x301, 1 << 7, [10, 10], 0xFFF),
    ] {
        let mut order = order();
        let mut patch = Plugin::new(FileHeader {
            masters: vec!["Skyrim.esm".into()],
            ..Default::default()
        });
        patch.push_top(record);
        order.add("Broken.esp", patch).unwrap();
        assert!(Footsteps::new(&order)
            .actor_sets(Some(id(0x10)), None, &BTreeSet::from([id(0x201)]))
            .is_err());
    }
}

#[test]
fn installed_skin_and_boot_sets_follow_the_authored_links() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let named = |tag: &[u8; 4], name: &str| {
        order
            .winners_of(T(tag))
            .find(|(_, record)| record.editor_id().as_deref() == Some(name))
            .unwrap()
            .0
    };
    let steps = Footsteps::new(&order);
    let nord = named(b"RACE", "NordRace");
    assert_eq!(
        steps
            .actor_sets(None, Some(nord), &BTreeSet::new())
            .unwrap(),
        vec![id(0x21468)]
    );
    for (addon, expected) in [(0x12E4A, 0x21487), (0x1B395, 0x21486)] {
        let armor = order
            .winners_of(T(b"ARMO"))
            .find(|(id, record)| {
                let file = order.winner_ref(*id).unwrap().file;
                record.all(T(b"MODL")).any(|bytes| {
                    let raw = u32::from_le_bytes(bytes.try_into().unwrap());
                    order.resolve(file, raw)
                        == GlobalFormId {
                            file: 0,
                            object: addon,
                        }
                })
            })
            .unwrap()
            .0;
        assert_eq!(
            steps
                .actor_sets(None, Some(nord), &BTreeSet::from([armor]))
                .unwrap(),
            vec![id(expected)]
        );
    }
    let wolf = named(b"RACE", "WolfRace");
    assert_eq!(
        steps
            .actor_sets(None, Some(wolf), &BTreeSet::new())
            .unwrap(),
        vec![id(0xDCE9D)]
    );
    let mut races = 0;
    let mut distinct = BTreeSet::new();
    for (race, record) in order.winners_of(T(b"RACE")) {
        if record.is_deleted() {
            continue;
        }
        distinct.extend(
            steps
                .actor_sets(None, Some(race), &BTreeSet::new())
                .unwrap_or_else(|error| panic!("race {race} {:?}: {error}", record.editor_id())),
        );
        races += 1;
    }
    assert!(
        races > 50 && distinct.len() > 20,
        "race survey is too narrow"
    );
    eprintln!(
        "{races} race skins resolved {} distinct sets",
        distinct.len()
    );
    eprintln!("Installed bare feet, heavy boots, light boots and wolf skin selected distinct authored sets");
}

#[test]
fn temporary_bases_keep_inherited_skin_and_gender_in_footstep_selection() {
    let mut plugin = plugin();
    let selected = plugin
        .records
        .iter_mut()
        .find(|r| r.form_id == 0x11)
        .unwrap();
    *selected = selected.clone().sub(T(b"WNAM"), 0x201_u32.to_le_bytes());
    let mut root = npc(0x12, false).sub(T(b"TPLT"), 0x110_u32.to_le_bytes());
    let acbs = root
        .subrecords
        .iter_mut()
        .find(|s| s.tag == T(b"ACBS"))
        .unwrap();
    acbs.data[18..20].copy_from_slice(&soe_game::npc::template::TRAITS.to_le_bytes());
    plugin.push_top(root);
    let mut lvlo = vec![0; 12];
    lvlo[4..8].copy_from_slice(&0x11_u32.to_le_bytes());
    lvlo[8..10].copy_from_slice(&1_u16.to_le_bytes());
    plugin.push_top(Record::new(T(b"LVLN"), 0x110).sub(T(b"LVLO"), lvlo));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let order = std::sync::Arc::new(order);
    let mut world = World::from_shared_order(std::sync::Arc::clone(&order));
    let actor = world.place_at(id(0x12), id(0x14), false);
    world.actor(actor).unwrap().equipped.insert(id(0x202));
    assert_eq!(world.footstep_sets(actor).unwrap(), [id(0x902), id(0x903)]);
    let temporary = world.leveled_actor_base(actor).unwrap().unwrap();
    assert_ne!(temporary, id(0x12));
    assert_ne!(temporary, id(0x11));
    assert_eq!(world.footstep_sets(actor).unwrap(), [id(0x902), id(0x903)]);
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(loaded.footstep_sets(actor).unwrap(), [id(0x902), id(0x903)]);
    let other = loaded.place_at(id(0x12), id(0x14), false);
    loaded.actor(other).unwrap().equipped.insert(id(0x202));
    loaded
        .set_actor_base_skin(temporary, Some(id(0x202)))
        .unwrap();
    assert_eq!(loaded.footstep_sets(actor).unwrap(), [id(0x903)]);
    assert_eq!(loaded.footstep_sets(other).unwrap(), [id(0x902), id(0x903)]);
}

#[test]
fn skin_overrides_change_shared_parts_without_carried_items() {
    let mut world = World::new(order());
    let actor = world.place_at(id(0x10), id(0x14), false);
    let other = world.place_at(id(0x10), id(0x14), false);
    let original = world.actor_appearance(actor).unwrap();
    world.actor_appearance(other).unwrap();
    let inventory = world.reference(actor).unwrap().inventory.clone();
    let equipped = world.actor(actor).unwrap().equipped.clone();
    assert_eq!(original.skin, Some(id(0x200)));
    world
        .set_actor_base_skin(id(0x10), Some(id(0x201)))
        .unwrap();
    for actor in [actor, other] {
        let view = world.actor_appearance(actor).unwrap();
        assert_eq!(view.skin, Some(id(0x201)));
        assert_eq!(view.parts.len(), 1);
        assert_eq!(view.parts[0].addon, id(0x301));
        assert_eq!(view.parts[0].visible_slots, (1 << 7) | (1 << 8));
        assert_eq!(world.footstep_sets(actor).unwrap(), [id(0x902)]);
    }
    world.set_race_skin(id(0x100), Some(id(0x202))).unwrap();
    assert_eq!(world.actor_appearance(actor).unwrap().skin, Some(id(0x201)));
    world.set_actor_base_skin(id(0x10), None).unwrap();
    assert_eq!(world.actor_appearance(actor).unwrap().skin, Some(id(0x202)));
    assert_eq!(world.footstep_sets(actor).unwrap(), [id(0x903)]);
    world.set_race_skin(id(0x100), None).unwrap();
    assert!(world.actor_appearance(actor).unwrap().parts.is_empty());
    assert_eq!(world.reference(actor).unwrap().inventory, inventory);
    assert_eq!(world.actor(actor).unwrap().equipped, equipped);
}

#[test]
fn appearance_exposes_sex_models_and_only_surviving_slots() {
    use soe_game::appearance::ActorAppearance;
    let mut plugin = plugin();
    for record in &mut plugin.records {
        if record.tag == T(b"ARMA") {
            *record = record
                .clone()
                .zsub(T(b"MOD2"), "male.nif")
                .zsub(T(b"MOD3"), "female.nif")
                .zsub(T(b"MOD4"), "arms_male.nif")
                .zsub(T(b"MOD5"), "arms_female.nif");
        }
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let equipped = BTreeSet::from([id(0x201), id(0x202)]);
    let female = ActorAppearance::from_order(&order, Some(id(0x11)), None, &equipped).unwrap();
    assert_eq!(female.parts.len(), 2);
    assert_eq!(female.parts[0].addon, id(0x301));
    assert_eq!(female.parts[0].authored_slots, (1 << 7) | (1 << 8));
    assert_eq!(female.parts[0].visible_slots, 1 << 8);
    assert_eq!(female.parts[1].visible_slots, 1 << 7);
    assert_eq!(female.parts[0].model.as_deref(), Some("female.nif"));
    assert_eq!(
        female.parts[0].first_person_model.as_deref(),
        Some("arms_female.nif")
    );
    let male = ActorAppearance::from_order(&order, Some(id(0x10)), None, &equipped).unwrap();
    assert_eq!(male.parts.len(), 1);
    assert_eq!(male.parts[0].model.as_deref(), Some("male.nif"));
}

#[test]
fn mapped_race_skin_survives_repeated_changes_and_save_reload() {
    let mut order = order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    patch.push_top(armor(0x0100_0800, 0x301));
    order.add("Skin.esp", patch).unwrap();
    let target = GlobalFormId {
        file: 1,
        object: 0x800,
    };
    let order = std::sync::Arc::new(order);
    let mut world = World::from_shared_order(order.clone());
    let actor = world.place_at(id(0x10), id(0x14), false);
    for skin in [Some(target), Some(id(0x202)), None, Some(target)] {
        world.set_race_skin(id(0x100), skin).unwrap();
        world.set_actor_base_skin(id(0x11), skin).unwrap();
    }
    let view = world.actor_appearance(actor).unwrap();
    assert_eq!(view.skin, Some(target));
    assert_eq!(view.parts[0].addon, id(0x301));
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(loaded.state_hash(), world.state_hash());
    assert_eq!(loaded.actor_appearance(actor).unwrap(), view);
    assert_eq!(loaded.skin(id(0x11)).unwrap(), Some(target));
    assert_eq!(
        world
            .order()
            .winner(id(0x100))
            .unwrap()
            .find(T(b"WNAM"))
            .unwrap(),
        0x200_u32.to_le_bytes()
    );
}

#[test]
fn invalid_skin_changes_and_tampered_race_overrides_are_atomic() {
    let mut world = World::new(order());
    let initial = world.state_hash();
    assert!(world.set_race_skin(id(0x100), Some(id(0x400))).is_err());
    assert!(world
        .set_actor_base_skin(id(0x100), Some(id(0x200)))
        .is_err());
    assert_eq!(world.state_hash(), initial);
    world.set_race_skin(id(0x100), Some(id(0x201))).unwrap();
    let before = world.state_hash();
    let mut bytes = world.save_state();
    let offset = bytes.windows(4).position(|v| v == b"WNAM").unwrap();
    bytes[offset..offset + 4].copy_from_slice(b"RNAM");
    assert!(world.load_state(&bytes).is_err());
    assert_eq!(world.state_hash(), before);
    world.set_race_skin(id(0x100), None).unwrap();
    let before = world.state_hash();
    let mut bytes = world.save_state();
    let field = [b"WNAM".as_slice(), &4_u32.to_le_bytes()].concat();
    let at = bytes
        .windows(field.len())
        .position(|bytes| bytes == field)
        .unwrap()
        + field.len();
    bytes[at..at + 4].copy_from_slice(&0xFEFF_F123_u32.to_le_bytes());
    assert!(
        world.load_state(&bytes).is_err(),
        "nonzero unresolved skin must be rejected"
    );
    assert_eq!(world.state_hash(), before);
}
