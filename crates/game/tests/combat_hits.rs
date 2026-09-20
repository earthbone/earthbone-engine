// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent WEAP/ARMO bytes, physical hit outcomes and CK event contracts.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{
        records::{Armor, Weapon},
        Error, MeleeHit,
    },
    inventory::{Extra, Stack},
    Game, Handle,
};
use soe_papyrus::{
    asm::{i, ident, FunctionBuilder, ScriptBuilder},
    Opcode::*,
    Value,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn weapon(object: u32, damage: u16, skill: i32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}
fn armor(object: u32, rating: u32, slots: u32, kind: u32) -> Record {
    let mut body = slots.to_le_bytes().to_vec();
    body.extend_from_slice(&kind.to_le_bytes());
    Record::new(Tag(*b"ARMO"), object)
        .sub(
            Tag(*b"DATA"),
            [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"DNAM"), rating.to_le_bytes())
        .sub(Tag(*b"BOD2"), body)
}
fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
}
fn fixture(extra: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    for record in extra {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.).unwrap();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}
fn uninitialized_fixture(extra: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for record in extra {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    Game::new(order)
}
fn hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
    }
}
fn equip(game: &Game, actor: u32, item: u32) {
    let mut world = game.world_mut();
    world.equip_item(id(actor), id(item)).unwrap();
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

#[test]
fn weapon_skill_is_independent_of_animation_and_fields_use_schema_offsets() {
    let parsed = Weapon::from_record(&weapon(0xA00, 27, 8)).unwrap();
    assert_eq!(
        (parsed.animation, parsed.skill, parsed.damage, parsed.resist),
        (1, 8, 27, -1)
    );
    assert_eq!(parsed.weight, 8.);
    assert_eq!(parsed.reach, 1.1);
    let parsed = Armor::from_record(&armor(0xB00, 70000, 4, 1)).unwrap();
    close(parsed.rating, 44.64);
}

#[test]
fn armor_templates_keep_child_value_and_enchantment_inherit_physical_fields() {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(
        Record::new(Tag(*b"ARMO"), 0xB00)
            .zsub(Tag(*b"FULL"), "Template")
            .sub(
                Tag(*b"DATA"),
                [40_i32.to_le_bytes(), 12_f32.to_le_bytes()].concat(),
            )
            .sub(Tag(*b"DNAM"), 7000_u32.to_le_bytes())
            .sub(
                Tag(*b"BOD2"),
                [4_u32.to_le_bytes(), 1_u32.to_le_bytes()].concat(),
            ),
    );
    master.push_top(Record::new(Tag(*b"ARMO"), 0xB01).sub(Tag(*b"TNAM"), 0xB00_u32.to_le_bytes()));
    let unrelated = Plugin::new(FileHeader::default());
    let other_master = Plugin::new(FileHeader::default());
    let mut child = Plugin::new(FileHeader {
        masters: vec!["Master.esm".into(), "Other.esm".into()],
        ..Default::default()
    });
    child.push_top(
        Record::new(Tag(*b"ARMO"), 0x0200_0800)
            .zsub(Tag(*b"FULL"), "Child")
            .sub(
                Tag(*b"DATA"),
                [90_i32.to_le_bytes(), 1_f32.to_le_bytes()].concat(),
            )
            .sub(Tag(*b"EITM"), 0x0200_0900_u32.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0x0000_0B01_u32.to_le_bytes()),
    );
    child.push_top(Record::new(Tag(*b"ENCH"), 0x0200_0900));
    let mut order = LoadOrder::new();
    order.add("Unrelated.esm", unrelated).unwrap();
    order.add("Master.esm", master).unwrap();
    order.add("Other.esm", other_master).unwrap();
    order.add("Child.esp", child).unwrap();
    let child_id = GlobalFormId {
        file: 3,
        object: 0x800,
    };
    assert_eq!(
        order.resolve(3, 0x0000_0B01),
        GlobalFormId {
            file: 1,
            object: 0xB01,
        }
    );
    let sources = Armor::sources(&order, child_id).unwrap();
    assert_eq!(sources.child, child_id);
    assert_eq!(
        sources.inherited,
        GlobalFormId {
            file: 1,
            object: 0xB00,
        }
    );
    let view = Armor::from_sources(
        order.winner(sources.child).unwrap(),
        order.winner(sources.inherited).unwrap(),
    )
    .unwrap();
    assert_eq!(view.value, Some(90));
    assert_eq!(view.weight, Some(12.));
    assert_eq!(view.rating, 70.);
    assert_eq!(view.slots, 4);
    assert_eq!(view.kind, Some(1));
    assert_eq!(view.enchantment, Some(0x0200_0900));
}

#[test]
fn armor_template_cycles_and_malformed_links_are_rejected() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(armor(0xB00, 0, 1, 0).sub(Tag(*b"TNAM"), 0x0000_0B01_u32.to_le_bytes()));
    plugin.push_top(armor(0xB01, 0, 1, 0).sub(Tag(*b"TNAM"), 0x0000_0B00_u32.to_le_bytes()));
    plugin.push_top(armor(0xB02, 0, 1, 0).sub(Tag(*b"TNAM"), [1, 2, 3]));
    let mut order = LoadOrder::new();
    order.add("Cycle.esm", plugin).unwrap();
    assert!(Armor::sources(&order, id(0xB00)).is_err());
    assert!(Armor::sources(&order, id(0xB02)).is_err());
}

#[test]
fn player_and_npc_skill_settings_are_distinct_and_overrides_are_live() {
    let mut game = fixture(vec![]);
    close(game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage, 15.);
    close(
        game.melee_hit(hit(0x900, 0x901)).unwrap().health_damage,
        30.,
    );
    game.world_mut()
        .settings
        .insert("fdamageweaponmult".into(), Value::Float(2.));
    close(game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage, 30.);
    close(game.world_mut().actor_value(id(0x901), "health"), 25.);
}

#[test]
fn armor_uses_its_skill_hidden_categories_and_reduction_cap() {
    let mut game = fixture(vec![armor(0xB00, 10000, 4, 1)]);
    equip(&game, 0x901, 0xB00);
    game.world_mut()
        .set_actor_value(id(0x901), "heavyarmor", 100.)
        .unwrap();
    let result = game.melee_hit(hit(0x14, 0x901)).unwrap();
    close(result.armor_rating, 250.);
    close(result.armor_reduction, 0.33);
    close(result.health_damage, 10.05);
    game.world_mut()
        .mod_actor_value(id(0x901), "damageresist", 1000.)
        .unwrap();
    close(game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage, 3.);
}

#[test]
fn equipping_overlapping_supported_armor_replaces_only_the_conflicting_item() {
    let game = fixture(vec![
        armor(0xB00, 100, 1 << 2, 1),
        armor(0xB01, 100, (1 << 2) | (1 << 3), 1),
        armor(0xB02, 100, 1 << 7, 1),
    ]);
    equip(&game, 0x901, 0xB00);
    equip(&game, 0x901, 0xB02);
    equip(&game, 0x901, 0xB01);
    let world = game.world();
    let equipped = &world.actor_ref(id(0x901)).unwrap().equipped;
    assert!(!equipped.contains(&id(0xB00)));
    assert!(equipped.contains(&id(0xB01)));
    assert!(equipped.contains(&id(0xB02)));
}

#[test]
fn equipping_templated_armor_uses_inherited_slots_for_conflicts() {
    let game = fixture(vec![
        armor(0xB00, 100, 1 << 2, 1),
        Record::new(Tag(*b"ARMO"), 0xB01)
            .sub(
                Tag(*b"DATA"),
                [20_i32.to_le_bytes(), 1_f32.to_le_bytes()].concat(),
            )
            .sub(Tag(*b"TNAM"), 0xB00_u32.to_le_bytes()),
        armor(0xB02, 100, 1 << 2, 1),
    ]);
    equip(&game, 0x901, 0xB02);
    game.world_mut().equip_item(id(0x901), id(0xB01)).unwrap();
    let mut world = game.world_mut();
    let actor = world.actor(id(0x901)).unwrap();
    assert!(actor.equipped.contains(&id(0xB01)));
    assert!(!actor.equipped.contains(&id(0xB02)));
}

#[test]
fn unsupported_bod2_overlap_is_not_replaced_and_combat_rejects_it() {
    let mut game = fixture(vec![
        armor(0xB00, 100, 1 << 31, 1),
        armor(0xB01, 100, 1 << 31, 1),
    ]);
    equip(&game, 0x901, 0xB00);
    equip(&game, 0x901, 0xB01);
    let world = game.world();
    let equipped = &world.actor_ref(id(0x901)).unwrap().equipped;
    assert!(equipped.contains(&id(0xB00)));
    assert!(equipped.contains(&id(0xB01)));
    drop(world);
    assert!(matches!(
        game.melee_hit(hit(0x14, 0x901)),
        Err(Error::Unsupported("overlapping equipped armor"))
    ));
}

#[test]
fn invalid_armor_skill_remains_an_error_after_equipping() {
    let mut game = fixture(vec![armor(0xB00, 100, 1 << 2, 3)]);
    assert!(matches!(
        game.world_mut().equip_item(id(0x901), id(0xB00)),
        Err(Error::Unsupported("unknown armor skill"))
    ));
    {
        let mut world = game.world_mut();
        world.add_item(id(0x901), id(0xB00), 1);
        world.actor(id(0x901)).unwrap().equipped.insert(id(0xB00));
    }
    assert!(matches!(
        game.melee_hit(hit(0x14, 0x901)),
        Err(Error::Unsupported("unknown armor skill"))
    ));
}

#[test]
fn equip_initializes_an_unloaded_actor_before_recording_armor() {
    let game = uninitialized_fixture(vec![armor(0xB00, 100, 1 << 2, 1)]);
    assert!(game.world().actor_ref(id(0x901)).is_none());
    equip(&game, 0x901, 0xB00);
    assert!(game
        .world()
        .actor_ref(id(0x901))
        .unwrap()
        .equipped
        .contains(&id(0xB00)));
}

#[test]
fn malformed_incoming_or_current_armor_leaves_equipment_and_inventory_unchanged() {
    let malformed = Record::new(Tag(*b"ARMO"), 0xB01).sub(Tag(*b"DNAM"), 100_u32.to_le_bytes());
    let game = fixture(vec![armor(0xB00, 100, 1 << 2, 1), malformed]);
    equip(&game, 0x901, 0xB00);
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.world_mut().equip_item(id(0x901), id(0xB01)),
        Err(Error::Record(_))
    ));
    assert_eq!(game.save_state().unwrap(), before);
    {
        let mut world = game.world_mut();
        world.add_item(id(0x901), id(0xB01), 1);
        world.actor(id(0x901)).unwrap().equipped.insert(id(0xB01));
    }
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.world_mut().equip_item(id(0x901), id(0xB00)),
        Err(Error::Record(_))
    ));
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn replacing_armor_clears_worn_metadata_and_survives_save_load() {
    let records = vec![armor(0xB00, 100, 1 << 2, 1), armor(0xB01, 100, 1 << 2, 1)];
    let game = fixture(records.clone());
    equip(&game, 0x901, 0xB00);
    game.world_mut()
        .reference(id(0x901))
        .unwrap()
        .inventory
        .restore(
            id(0xB00),
            1,
            vec![Stack::new(
                1,
                vec![Extra {
                    kind: 22,
                    fields: vec![],
                }],
            )
            .unwrap()],
        )
        .unwrap();
    equip(&game, 0x901, 0xB01);
    assert!(!game
        .world()
        .reference_ref(id(0x901))
        .unwrap()
        .inventory
        .stacks(id(0xB00))
        .iter()
        .any(Stack::worn));
    let saved = game.save_state().unwrap();
    let mut restored = fixture(records);
    restored.load_state(&saved).unwrap();
    let world = restored.world();
    let actor = world.actor_ref(id(0x901)).unwrap();
    assert!(!actor.equipped.contains(&id(0xB00)));
    assert!(actor.equipped.contains(&id(0xB01)));
    assert!(!restored
        .world()
        .reference_ref(id(0x901))
        .unwrap()
        .inventory
        .stacks(id(0xB00))
        .iter()
        .any(Stack::worn));
}

#[test]
fn reequipping_armor_preserves_worn_metadata_and_saved_state() {
    let game = fixture(vec![armor(0xB00, 100, 1 << 2, 1)]);
    equip(&game, 0x901, 0xB00);
    game.world_mut()
        .reference(id(0x901))
        .unwrap()
        .inventory
        .restore(
            id(0xB00),
            1,
            vec![Stack::new(
                1,
                vec![Extra {
                    kind: 22,
                    fields: vec![],
                }],
            )
            .unwrap()],
        )
        .unwrap();
    let before = game.save_state().unwrap();
    equip(&game, 0x901, 0xB00);
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn clothing_does_not_supply_rating_or_hidden_armor_and_absent_items_are_rejected() {
    let mut game = fixture(vec![armor(0xB00, 10000, 4, 2)]);
    equip(&game, 0x901, 0xB00);
    close(game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage, 15.);
    game.world_mut()
        .reference(id(0x901))
        .unwrap()
        .inventory
        .clear();
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.melee_hit(hit(0x14, 0x901)),
        Err(Error::Equipment)
    ));
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn invalid_settings_and_unsupported_perks_do_not_apply_partial_damage_or_events() {
    let mut game = fixture(vec![]);
    game.world_mut().actor_base(id(0x800)).unwrap();
    for value in [f32::NAN, -1., f32::INFINITY] {
        game.world_mut()
            .settings
            .insert("farmorscalingfactor".into(), Value::Float(value));
        let before = game.save_state().unwrap();
        assert!(matches!(
            game.melee_hit(hit(0x14, 0x901)),
            Err(Error::Setting("fArmorScalingFactor"))
        ));
        assert_eq!(game.save_state().unwrap(), before);
    }
    game.world_mut().settings.remove("farmorscalingfactor");
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .perks
        .insert(id(0xDEAD));
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.melee_hit(hit(0x14, 0x901)),
        Err(Error::Unsupported("perk entry points"))
    ));
    assert_eq!(game.save_state().unwrap(), before);
}

fn observer(game: &mut Game) -> soe_papyrus::ObjectId {
    let mut script = ScriptBuilder::new("HitObserver", "Actor").variable("sequence", "Int", i(0));
    for (name, digit) in [("OnHit", 1), ("OnDying", 2), ("OnDeath", 3)] {
        let mut function = FunctionBuilder::new(name, "None")
            .param("actor", "Actor")
            .local("next", "Int");
        if name == "OnHit" {
            function = function
                .param("source", "Form")
                .param("projectile", "Projectile");
            for param in ["power", "sneak", "bash", "blocked"] {
                function = function.param(param, "Bool");
            }
        }
        script = script.function(
            function
                .ins(IMul, vec![ident("next"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("next"), i(digit)])
                .build(),
        );
    }
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of(Handle::Form(id(0x901)));
    game.vm.attach(object, "HitObserver").unwrap();
    object
}

#[test]
fn queued_hit_precedes_death_and_survives_game_save_without_reapplying_damage() {
    let mut game = fixture(vec![]);
    let object = observer(&mut game);
    game.world_mut()
        .set_actor_value(id(0x901), "health", 10.)
        .unwrap();
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .melee_hit(hit(0x14, 0x901), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    assert!(result.killed);
    assert_eq!(
        game.world().actor_ref(id(0x901)).unwrap().killer,
        Some(id(0x14))
    );
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(
        game.vm.get_var(object, "HitObserver", "sequence"),
        Some(Value::Int(123))
    );
    close(game.world_mut().actor_value(id(0x901), "health"), -5.);
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    assert!(game.run_until_idle(1));
    assert_eq!(
        game.vm.get_var(object, "HitObserver", "sequence"),
        Some(Value::Int(123))
    );
}

#[test]
fn deferred_melee_death_retains_the_attacker() {
    let mut game = fixture(vec![]);
    let object = observer(&mut game);
    game.world_mut()
        .set_actor_value(id(0x901), "health", 10.)
        .unwrap();
    game.world_mut().start_deferred_kill(id(0x901));
    assert!(!game.melee_hit(hit(0x14, 0x901)).unwrap().killed);
    assert_eq!(
        game.vm.get_var(object, "HitObserver", "sequence"),
        Some(Value::Int(1))
    );
    assert!(game.world_mut().end_deferred_kill(id(0x901)));
    assert!(game.run_until_idle(32));
    assert_eq!(
        game.world().actor_ref(id(0x901)).unwrap().killer,
        Some(id(0x14))
    );
    assert_eq!(
        game.vm.get_var(object, "HitObserver", "sequence"),
        Some(Value::Int(123))
    );
}

#[test]
fn weapon_record_truncations_are_rejected() {
    let record = weapon(0xA00, 10, 6);
    for tag in [Tag(*b"DATA"), Tag(*b"DNAM")] {
        let bytes = record.find(tag).unwrap();
        for length in 0..bytes.len() {
            let mut bad = record.clone();
            for field in &mut bad.subrecords {
                if field.tag == tag {
                    field.data = bytes[..length].to_vec();
                }
            }
            assert!(Weapon::from_record(&bad).is_err());
        }
    }
}

#[test]
fn npc_hits_baseless_player_and_essential_attacker_can_deal_damage() {
    let mut game = fixture(vec![]);
    game.world_mut().actor(id(0x900)).unwrap().essential = true;
    close(game.melee_hit(hit(0x900, 0x14)).unwrap().health_damage, 30.);
    close(game.world_mut().actor_value(id(0x14), "health"), 70.);
}

#[test]
fn installed_weapon_and_armor_fields_decode_and_ordinary_weapons_execute() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping weapon/armor survey");
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
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let mut weapons = Vec::new();
    let mut armors = 0;
    for (form, record) in order
        .winners_of(Tag(*b"WEAP"))
        .filter(|(_, record)| !record.is_deleted())
    {
        weapons.push((form, Weapon::from_record(record).unwrap()));
    }
    for (_, record) in order
        .winners_of(Tag(*b"ARMO"))
        .filter(|(_, record)| !record.is_deleted())
    {
        Armor::from_record(record).unwrap();
        armors += 1;
    }
    assert!(weapons.len() > 1000 && armors > 1000);
    // The real weapons run against synthetic actors to isolate their damage inputs.
    let file = 5;
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x01000800));
    for actor in [0x01000900, 0x01000901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x01000800_u32.to_le_bytes()),
        );
    }
    order.add("HitSurvey.esp", plugin).unwrap();
    let attacker = GlobalFormId {
        file,
        object: 0x900,
    };
    let target = GlobalFormId {
        file,
        object: 0x901,
    };
    let mut game = Game::new(order);
    for actor in [attacker, target] {
        game.world_mut()
            .set_actor_value(actor, "health", 10000.)
            .unwrap();
    }
    let mut accepted = 0;
    let mut excluded = std::collections::BTreeMap::new();
    for (form, _) in &weapons {
        game.world_mut().add_item(attacker, *form, 1);
        game.world_mut().actor(attacker).unwrap().equipped.clear();
        game.world_mut()
            .actor(attacker)
            .unwrap()
            .equipped
            .insert(*form);
        game.world_mut().resurrect_actor(target).unwrap();
        match game.melee_hit(MeleeHit {
            attacker,
            target,
            weapon: *form,
        }) {
            Ok(result) => {
                assert!(result.health_damage.is_finite());
                accepted += 1;
            }
            Err(Error::Unsupported(reason)) => {
                *excluded.entry(reason).or_insert(0) += 1;
            }
            Err(error) => panic!("{form}: {error}"),
        }
    }
    assert!(
        accepted > 100,
        "too few real melee weapons executed: {accepted}"
    );
    eprintln!(
        "Combat records: {} weapons, {armors} armors. Executed {accepted}, excluded {excluded:?}",
        weapons.len()
    );
}

#[test]
fn absent_engine_settings_use_defaults_but_plugin_and_runtime_overrides_win() {
    let empty = Game::new(LoadOrder::new());
    assert_eq!(
        empty.world().game_setting("fDamageSkillMax"),
        Some(Value::Float(3.))
    );
    assert_eq!(empty.world().game_setting("fUnknownCombatSetting"), None);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"GMST"), 0x800)
            .zsub(Tag(*b"EDID"), "fDamageSkillMax")
            .sub(Tag(*b"DATA"), 4_f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Settings.esp", plugin).unwrap();
    let game = Game::new(order);
    assert_eq!(
        game.world().game_setting("fDamageSkillMax"),
        Some(Value::Float(4.))
    );
    game.world_mut()
        .settings
        .insert("fdamageskillmax".into(), Value::Float(5.));
    assert_eq!(
        game.world().game_setting("fDamageSkillMax"),
        Some(Value::Float(5.))
    );
}

#[test]
fn adept_difficulty_overrides_affect_only_hits_involving_the_player() {
    let mut game = fixture(vec![]);
    game.world_mut()
        .settings
        .insert("fdiffmulthpbypcn".into(), Value::Float(0.5));
    game.world_mut()
        .settings
        .insert("fdiffmulthptopcn".into(), Value::Float(2.));
    close(game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage, 7.5);
    close(game.melee_hit(hit(0x900, 0x14)).unwrap().health_damage, 60.);
    close(
        game.melee_hit(hit(0x900, 0x901)).unwrap().health_damage,
        30.,
    );
}
