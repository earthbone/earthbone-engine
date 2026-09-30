// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Independent WEAP/ARMO bytes, physical hit outcomes and CK event contracts.

#[path = "common/installed.rs"]
mod installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{
        records::{Armor, Weapon},
        Difficulty, Error, MeleeHit,
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
fn unarmed_weapon() -> Record {
    // The source identity selects race damage. This fixture only supplies a
    // complete WEAP layout, while the installed survey checks Skyrim.esm's form.
    weapon(0x1F4, 0, -1)
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
fn weapon_template(object: u32, template: u32) -> Record {
    Record::new(Tag(*b"WEAP"), object).sub(Tag(*b"CNAM"), template.to_le_bytes())
}
fn armor_template(object: u32, template: u32) -> Record {
    Record::new(Tag(*b"ARMO"), object).sub(Tag(*b"TNAM"), template.to_le_bytes())
}
fn race(object: u32, unarmed_damage: f32) -> Record {
    let mut data = vec![0; 124];
    put(&mut data, 96, unarmed_damage.to_bits());
    Record::new(Tag(*b"RACE"), object).sub(Tag(*b"DATA"), data)
}
fn templated_npc(object: u32, template: u32) -> Record {
    let mut acbs = vec![0; 24];
    acbs[18..20].copy_from_slice(&1_u16.to_le_bytes());
    Record::new(Tag(*b"NPC_"), object)
        .sub(Tag(*b"ACBS"), acbs)
        .sub(Tag(*b"TPLT"), template.to_le_bytes())
}
fn npc_with_race(object: u32, race: u32) -> Record {
    Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"RNAM"), race.to_le_bytes())
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
fn unarmed_game(winning_race: Record) -> Game {
    unarmed_game_with_source(winning_race, Some(unarmed_weapon()))
}
fn unarmed_game_with_source(winning_race: Record, source: Option<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 4.));
    plugin.push_top(templated_npc(0x800, 0x801));
    plugin.push_top(npc_with_race(0x801, 0x700));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    if let Some(source) = source {
        plugin.push_top(source);
    }
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Combat.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(winning_race);
    order.add("CombatPatch.esp", patch).unwrap();
    Game::new(order)
}
fn ready_unarmed_game(winning_race: Record) -> Game {
    let game = unarmed_game(winning_race);
    game.world_mut().actor_base(id(0x800)).unwrap();
    for actor in [0x900, 0x901] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
    }
    game
}
fn ready_unarmed_game_with_source(winning_race: Record, source: Option<Record>) -> Game {
    let game = unarmed_game_with_source(winning_race, source);
    game.world_mut().actor_base(id(0x800)).unwrap();
    for actor in [0x900, 0x901] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
    }
    game
}
fn hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}
fn unarmed_hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0x1F4),
        is_bash: false,
        is_power_attack: false,
    }
}
fn equip(game: &Game, actor: u32, item: u32) {
    let mut world = game.world_mut();
    world.equip_item(id(actor), id(item)).unwrap();
}
fn warm_combat_actor_base(game: &mut Game) {
    // Combat builds this view while checking target base flags.
    game.world_mut().actor_base(id(0x800)).unwrap();
}
fn template_fixture(
    template: Record,
    sources: Vec<Record>,
    patch_records: Vec<Record>,
) -> (Game, GlobalFormId) {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    master.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for source in sources {
        master.push_top(source);
    }
    settings(&mut master);
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Combat.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(template);
    for record in patch_records {
        patch.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Combat.esm", master).unwrap();
    order.add("Template.esp", patch).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x901)] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(actor, "onehanded", 100.)
            .unwrap();
    }
    let item = GlobalFormId {
        file: 1,
        object: 0xA01,
    };
    game.world_mut().add_item(id(0x14), item, 1);
    game.world_mut()
        .actor(id(0x14))
        .unwrap()
        .equipped
        .insert(item);
    (game, item)
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

#[test]
fn mag_34_melee_hit_ends_attacker_invisibility() {
    let mut data = vec![0; 152];
    put(&mut data, 64, 11);
    put(&mut data, 80, 1);
    let mut spell_data = vec![0; 36];
    put(&mut spell_data, 16, 1);
    let invisibility = Record::new(Tag(*b"MGEF"), 0xB10).sub(Tag(*b"DATA"), data);
    let spell = Record::new(Tag(*b"SPEL"), 0xB11)
        .sub(Tag(*b"SPIT"), spell_data)
        .sub(Tag(*b"EFID"), 0xB10_u32.to_le_bytes())
        .sub(
            Tag(*b"EFIT"),
            [
                1.0_f32.to_le_bytes(),
                0_u32.to_le_bytes(),
                20_u32.to_le_bytes(),
            ]
            .concat(),
        );
    let mut game = fixture(vec![invisibility, spell]);
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(0xB11),
            id(0x14),
            Some(id(0x14)),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert!(game.world().is_invisible(id(0x14)));

    game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(!game.world().is_invisible(id(0x14)));
}

#[test]
fn cmb_15_attack_damage_mult_uses_shared_components_and_survives_save() {
    let records = vec![armor(0xB00, 10_000, 1 << 2, 1)];
    let mut game = fixture(records.clone());
    let player = game.player();
    close(game.world_mut().actor_value(player, "AttackDamageMult"), 1.);
    game.world_mut()
        .mod_actor_value(player, "attackdamagemult", 0.5)
        .unwrap();
    game.world_mut()
        .mod_temporary_actor_value(player, "attackdamagemult", 0.25)
        .unwrap();
    game.world_mut()
        .damage_actor_value(player, "attackdamagemult", 0.25)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(0x901), "attackdamagemult", 10.)
        .unwrap();
    equip(&game, 0x901, 0xB00);
    game.set_difficulty(Difficulty::Expert);
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(records);
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.world().state_hash(), game.world().state_hash());
    let result = loaded.melee_hit(hit(0x14, 0x901)).unwrap();
    close(result.weapon_damage, 22.5);
    close(result.armor_rating, 100.);
    close(result.armor_reduction, 0.15);
    close(result.health_damage, 22.5 * 0.85 * 0.75);
}

#[test]
fn cmb_15_explicit_zero_mult_survives_save_and_only_scales_outgoing_damage() {
    let game = fixture(vec![]);
    game.world_mut()
        .set_actor_value(id(0x900), "attackdamagemult", 0.)
        .unwrap();
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(vec![]);
    loaded.load_state(&saved).unwrap();
    close(
        loaded.melee_hit(hit(0x900, 0x901)).unwrap().health_damage,
        0.,
    );
    close(
        loaded.melee_hit(hit(0x901, 0x900)).unwrap().health_damage,
        30.,
    );
}

#[test]
fn cmb_15_unarmed_damage_uses_attack_damage_mult() {
    let mut game = ready_unarmed_game(race(0x700, 10.));
    game.world_mut()
        .mod_actor_value(id(0x900), "attackdamagemult", 0.5)
        .unwrap();
    close(
        game.melee_hit(unarmed_hit(0x900, 0x901))
            .unwrap()
            .health_damage,
        15.,
    );
}

#[test]
fn cmb_15_invalid_damage_mult_rejects_without_health_or_event_changes() {
    for value in [-1., f32::MAX] {
        let mut game = fixture(vec![]);
        warm_combat_actor_base(&mut game);
        game.world_mut()
            .set_actor_value(id(0x14), "attackdamagemult", value)
            .unwrap();
        let before = game.save_state().unwrap();
        assert!(matches!(
            game.melee_hit(hit(0x14, 0x901)),
            Err(Error::Number)
        ));
        assert_eq!(game.save_state().unwrap(), before);
    }
}

#[test]
fn cmb_15_perk_driven_combat_modifiers_reject_without_applying_a_hit() {
    let names = [
        "onehandedmodifier",
        "twohandedmodifier",
        "marksmanmodifier",
        "onehandedpowermodifier",
        "twohandedpowermodifier",
        "marksmanpowermodifier",
        "lightarmormodifier",
        "heavyarmormodifier",
        "lightarmorpowermodifier",
        "heavyarmorpowermodifier",
    ];
    for actor in [0x14, 0x901] {
        for name in names {
            let mut game = fixture(vec![]);
            warm_combat_actor_base(&mut game);
            game.world_mut()
                .set_actor_value(id(actor), name, 10.)
                .unwrap();
            let before = game.save_state().unwrap();
            assert!(
                matches!(
                    game.melee_hit(hit(0x14, 0x901)),
                    Err(Error::Unsupported("combat actor-value multipliers"))
                ),
                "{name} on actor {actor:#x} must remain gated until its PERK effects are evaluated"
            );
            assert_eq!(game.save_state().unwrap(), before, "{name} changed state");
        }
    }
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
fn unarmed_uses_the_winning_trait_race_and_current_actor_value() {
    let mut game = ready_unarmed_game(race(0x700, 10.));
    close(
        game.melee_hit(unarmed_hit(0x900, 0x901))
            .unwrap()
            .health_damage,
        10.,
    );
    game.world_mut()
        .mod_actor_value(id(0x900), "unarmeddamage", 2.)
        .unwrap();
    let bytes = game.save_state().unwrap();
    let mut restored = ready_unarmed_game(race(0x700, 10.));
    restored.load_state(&bytes).unwrap();
    close(
        restored
            .melee_hit(unarmed_hit(0x900, 0x901))
            .unwrap()
            .health_damage,
        12.,
    );
}

#[test]
fn unarmed_composes_with_templated_armor_difficulty_and_saved_state() {
    let records = vec![
        unarmed_weapon(),
        armor_template(0xB00, 0xB01),
        armor(0xB01, 10_000, 1 << 2, 1),
    ];
    let mut game = fixture(records.clone());
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "unarmeddamage", 10.)
        .unwrap();
    equip(&game, 0x901, 0xB00);
    game.set_difficulty(Difficulty::Legendary);
    let result = game.melee_hit(unarmed_hit(0x14, 0x901)).unwrap();
    close(result.weapon_damage, 10.);
    close(result.armor_rating, 100.);
    close(result.armor_reduction, 0.15);
    close(result.health_damage, 2.125);
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(records);
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.world().state_hash(), game.world().state_hash());
    assert_eq!(loaded.difficulty(), Difficulty::Legendary);
    close(
        loaded
            .melee_hit(unarmed_hit(0x14, 0x901))
            .unwrap()
            .health_damage,
        2.125,
    );
}

#[test]
fn unarmed_source_does_not_require_inventory_or_an_empty_weapon_slot() {
    let mut game = ready_unarmed_game(race(0x700, 10.));
    {
        let mut world = game.world_mut();
        world.add_item(id(0x900), id(0xA00), 1);
        world.actor(id(0x900)).unwrap().equipped.insert(id(0xA00));
    }
    close(
        game.melee_hit(unarmed_hit(0x900, 0x901))
            .unwrap()
            .health_damage,
        10.,
    );
}

#[test]
fn unarmed_requires_a_live_canonical_weapon_record_without_mutating_state() {
    for source in [
        None,
        Some(Record::new(Tag(*b"ARMO"), 0x1F4)),
        Some({
            let mut record = unarmed_weapon();
            record.flags = soe_esm::plugin::flags::DELETED;
            record
        }),
        Some(Record::new(Tag(*b"WEAP"), 0x1F4)),
    ] {
        let mut game = ready_unarmed_game_with_source(race(0x700, 10.), source);
        let before = game.save_state().unwrap();
        assert!(matches!(
            game.melee_hit(unarmed_hit(0x900, 0x901)),
            Err(Error::Equipment | Error::Record(_))
        ));
        assert_eq!(game.save_state().unwrap(), before);
    }
}

#[test]
fn unarmed_rejects_unsupported_canonical_weapon_modifiers() {
    let mut flagged = unarmed_weapon();
    flagged
        .subrecords
        .iter_mut()
        .find(|field| field.tag == Tag(*b"DNAM"))
        .unwrap()
        .data[12..14]
        .copy_from_slice(&1_u16.to_le_bytes());
    let mut resistant = unarmed_weapon();
    put(
        &mut resistant
            .subrecords
            .iter_mut()
            .find(|field| field.tag == Tag(*b"DNAM"))
            .unwrap()
            .data,
        88,
        0,
    );
    // CMB-26: the "Ignores Normal Weapon Resistance" flag no longer rejects
    // the weapon, it only suppresses the resistance reduction term, so
    // `flagged` (an otherwise ordinary unarmed contact) now succeeds instead
    // of joining the rejection loop below.
    let mut ignoring = ready_unarmed_game_with_source(race(0x700, 10.), Some(flagged));
    assert!(ignoring.melee_hit(unarmed_hit(0x900, 0x901)).is_ok());
    for source in [
        unarmed_weapon().sub(Tag(*b"EITM"), 0x123_u32.to_le_bytes()),
        resistant,
    ] {
        let mut game = ready_unarmed_game_with_source(race(0x700, 10.), Some(source));
        assert!(matches!(
            game.melee_hit(unarmed_hit(0x900, 0x901)),
            Err(Error::Unsupported(
                "weapon template or enchantment" | "unsupported weapon resistance value"
            ))
        ));
    }
}

#[test]
fn malformed_unarmed_race_rejects_the_hit() {
    let mut game = unarmed_game(Record::new(Tag(*b"RACE"), 0x700).sub(Tag(*b"DATA"), [0; 96]));
    game.world_mut().actor_base(id(0x800)).unwrap();
    assert!(game.world_mut().initial_actor_values(id(0x800)).is_err());
    assert!(matches!(
        game.melee_hit(unarmed_hit(0x900, 0x901)),
        Err(Error::Actor(actor)) if actor == id(0x900)
    ));
}

#[test]
fn installed_race_backed_unarmed_hits_execute() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping race-backed unarmed survey");
        return;
    };
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let mut order = installed::owned_order(&masters).unwrap();
    let races: Vec<_> = order
        .winners_of(Tag(*b"RACE"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(form, _)| form)
        .collect();
    assert!(
        races.len() > 100,
        "too few installed races: {}",
        races.len()
    );
    let mut fixture = Plugin::new(FileHeader {
        masters: masters.into_iter().map(str::to_owned).collect(),
        ..FileHeader::default()
    });
    for (index, race) in races.iter().enumerate() {
        let base = 0x0500_1000 + index as u32;
        let attacker = 0x0500_2000 + index as u32 * 2;
        fixture.push_top(
            Record::new(Tag(*b"NPC_"), base)
                .sub(Tag(*b"RNAM"), order.runtime_id(*race).to_le_bytes()),
        );
        for reference in [attacker, attacker + 1] {
            fixture.push_top(
                Record::new(Tag(*b"ACHR"), reference).sub(Tag(*b"NAME"), base.to_le_bytes()),
            );
        }
    }
    order.add("UnarmedSurvey.esp", fixture).unwrap();
    let mut game = Game::new(order);
    let mut executed = 0;
    for index in 0..races.len() {
        let attacker = GlobalFormId {
            file: 5,
            object: 0x2000 + index as u32 * 2,
        };
        let target = GlobalFormId {
            file: 5,
            object: 0x2001 + index as u32 * 2,
        };
        for actor in [attacker, target] {
            game.world_mut()
                .set_actor_value(actor, "health", 10000.)
                .unwrap();
        }
        let result = game
            .melee_hit(MeleeHit {
                attacker,
                target,
                weapon: id(0x1F4),
                is_bash: false,
                is_power_attack: false,
            })
            .unwrap();
        assert!(result.health_damage.is_finite());
        executed += 1;
    }
    assert_eq!(executed, races.len());
    eprintln!("Executed {executed} installed race-backed unarmed contacts");
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
fn weapon_and_armor_templates_supply_combat_values() {
    let mut game = fixture(vec![
        weapon_template(0xA01, 0xA02),
        weapon(0xA02, 20, 6),
        armor_template(0xB00, 0xB01),
        armor(0xB01, 10000, 4, 1),
    ]);
    equip(&game, 0x14, 0xA01);
    equip(&game, 0x901, 0xB00);
    game.world_mut()
        .set_actor_value(id(0x901), "heavyarmor", 100.)
        .unwrap();
    let result = game
        .melee_hit(MeleeHit {
            weapon: id(0xA01),
            ..hit(0x14, 0x901)
        })
        .unwrap();
    close(result.weapon_damage, 30.);
    close(result.armor_rating, 250.);
    close(result.health_damage, 20.1);
}

#[test]
fn a_fieldless_cross_plugin_weapon_template_uses_the_winning_master_record() {
    let (mut game, weapon) = template_fixture(
        weapon_template(0x0100_0A01, 0x0000_0A02),
        vec![weapon(0xA02, 20, 6)],
        vec![weapon(0xA02, 25, 6)],
    );
    let result = game
        .melee_hit(MeleeHit {
            attacker: id(0x14),
            target: id(0x901),
            weapon,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();
    close(result.weapon_damage, 37.5);
    close(result.health_damage, 37.5);
}

#[test]
fn a_fieldless_cross_plugin_armor_template_uses_the_winning_master_record() {
    let (mut game, weapon) = template_fixture(
        weapon_template(0x0100_0A01, 0x0000_0A02),
        vec![weapon(0xA02, 20, 6), armor(0xB01, 10000, 4, 1)],
        vec![
            armor_template(0x0100_0B00, 0x0000_0B01),
            armor(0xB01, 12000, 4, 1),
        ],
    );
    let armor = GlobalFormId {
        file: 1,
        object: 0xB00,
    };
    game.world_mut().add_item(id(0x901), armor, 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(armor);
    game.world_mut()
        .set_actor_value(id(0x901), "heavyarmor", 100.)
        .unwrap();
    let result = game
        .melee_hit(MeleeHit {
            attacker: id(0x14),
            target: id(0x901),
            weapon,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();
    close(result.armor_rating, 300.);
    close(result.health_damage, 18.3);
}

#[test]
fn cyclic_weapon_template_rejects_without_damage() {
    let mut game = fixture(vec![
        weapon_template(0xA01, 0xA02),
        weapon_template(0xA02, 0xA01),
    ]);
    equip(&game, 0x14, 0xA01);
    warm_combat_actor_base(&mut game);
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.melee_hit(MeleeHit {
            weapon: id(0xA01),
            ..hit(0x14, 0x901)
        }),
        Err(Error::Unsupported("cyclic weapon template"))
    ));
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn cyclic_armor_template_rejects_without_damage_or_events() {
    let mut game = fixture(vec![
        armor_template(0xB00, 0xB01),
        armor_template(0xB01, 0xB00),
    ]);
    game.world_mut().add_item(id(0x901), id(0xB00), 1);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));
    warm_combat_actor_base(&mut game);
    let before = game.save_state().unwrap();
    assert!(matches!(
        game.melee_hit(hit(0x14, 0x901)),
        Err(Error::Unsupported("cyclic armor template"))
    ));
    assert_eq!(game.save_state().unwrap(), before);
}

#[test]
fn unresolved_or_wrong_type_template_rejects_without_damage_or_events() {
    for (template, source) in [
        (weapon_template(0xA01, 0xDEAD), None),
        (
            weapon_template(0xA01, 0xB00),
            Some(armor(0xB00, 10000, 4, 1)),
        ),
        (
            Record::new(Tag(*b"WEAP"), 0xA01).sub(Tag(*b"CNAM"), [0, 0, 0]),
            None,
        ),
    ] {
        let mut records = vec![template];
        if let Some(source) = source {
            records.push(source);
        }
        let mut game = fixture(records);
        equip(&game, 0x14, 0xA01);
        warm_combat_actor_base(&mut game);
        let before = game.save_state().unwrap();
        assert!(game
            .melee_hit(MeleeHit {
                weapon: id(0xA01),
                ..hit(0x14, 0x901)
            })
            .is_err());
        assert_eq!(game.save_state().unwrap(), before);
    }
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
fn invalid_settings_reject_but_unresolved_perk_identity_keeps_base_damage() {
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
    let result = game.melee_hit(hit(0x14, 0x901)).unwrap();
    close(result.weapon_damage, 15.);
    close(result.health_damage, 15.);
    assert_ne!(game.save_state().unwrap(), before);
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
        .melee_hit(
            hit(0x14, 0x901),
            Vec::new(),
            Vec::new(),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
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
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping weapon/armor survey");
        return;
    };
    let mut order = installed::owned_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
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
            is_bash: false,
            is_power_attack: false,
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
fn tst_14_combat_hits_reuses_the_installed_parse_and_isolates_games() {
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let Some(order) = installed::load_order(&masters) else {
        eprintln!("SKYRIM_DATA not set, skipping TST-14 installed parse check");
        return;
    };
    let cached = installed::load_order(&masters).unwrap();
    assert!(std::sync::Arc::ptr_eq(&order, &cached));

    let first = Game::from_shared_order(order.clone());
    let second = Game::from_shared_order(cached);
    first.world_mut().perk_points = 7;
    assert_eq!(first.world().perk_points, 7);
    assert_eq!(second.world().perk_points, 0);
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
fn every_difficulty_uses_its_documented_player_damage_multipliers() {
    for (difficulty, from_player, to_player) in [
        (Difficulty::Novice, 30., 15.),
        (Difficulty::Apprentice, 22.5, 22.5),
        (Difficulty::Adept, 15., 30.),
        (Difficulty::Expert, 11.25, 45.),
        (Difficulty::Master, 7.5, 60.),
        (Difficulty::Legendary, 3.75, 90.),
    ] {
        let mut game = fixture(vec![]);
        game.set_difficulty(difficulty);
        close(
            game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage,
            from_player,
        );
        close(
            game.melee_hit(hit(0x900, 0x14)).unwrap().health_damage,
            to_player,
        );
        close(
            game.melee_hit(hit(0x900, 0x901)).unwrap().health_damage,
            30.,
        );
    }
}

#[test]
fn selected_difficulty_reads_its_live_gmst_overrides_and_survives_save_load() {
    let mut game = fixture(vec![]);
    let adept_hash = game.world().state_hash();
    game.set_difficulty(Difficulty::Legendary);
    assert_ne!(game.world().state_hash(), adept_hash);
    game.world_mut()
        .settings
        .insert("fdiffmulthpbypcl".into(), Value::Float(0.5));
    game.world_mut()
        .settings
        .insert("fdiffmulthptopcl".into(), Value::Float(2.));
    close(game.melee_hit(hit(0x14, 0x901)).unwrap().health_damage, 7.5);
    close(game.melee_hit(hit(0x900, 0x14)).unwrap().health_damage, 60.);
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(vec![]);
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.difficulty(), Difficulty::Legendary);
    assert_eq!(loaded.world().state_hash(), game.world().state_hash());
    close(
        loaded.melee_hit(hit(0x14, 0x901)).unwrap().health_damage,
        7.5,
    );
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
fn equipping_fieldless_armor_templates_resolves_slot_conflicts() {
    let game = fixture(vec![
        armor(0xB00, 100, 1 << 2, 1),
        Record::new(Tag(*b"ARMO"), 0xB01).sub(Tag(*b"TNAM"), 0xB02_u32.to_le_bytes()),
        armor(0xB02, 200, (1 << 2) | (1 << 3), 1),
    ]);
    equip(&game, 0x901, 0xB00);
    equip(&game, 0x901, 0xB01);
    let world = game.world();
    let equipped = &world.actor_ref(id(0x901)).unwrap().equipped;
    assert!(!equipped.contains(&id(0xB00)));
    assert!(equipped.contains(&id(0xB01)));
    assert!(!equipped.contains(&id(0xB02)));
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
fn cmb_05_block_reduction_uses_record_backed_multipliers_and_survives_save_load() {
    // DNAM stores the rating in hundredths, so 1000 decodes to a 10.0 rating.
    let extra = vec![armor(0xB00, 1000, 1 << 9, 2)];
    let mut game = fixture(extra.clone());
    equip(&game, 0x901, 0xB00);
    game.world_mut()
        .set_actor_value(id(0x901), "block", 50.)
        .unwrap();

    // A shield in the BOD2 shield slot contributes nothing while the target
    // is not in a block stance.
    let unblocked = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(!unblocked.blocked);
    close(unblocked.block_reduction, 0.);
    close(unblocked.health_damage, 15.);

    // Blocking with a shield equipped: Block record-backed multipliers apply.
    game.world_mut().set_blocking(id(0x901), true).unwrap();
    let blocked = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(blocked.blocked);
    close(blocked.block_reduction, 0.48);
    close(blocked.health_damage, 7.8);

    // Weapon-only blocking, with no shield in the BOD2 shield slot, is not
    // evaluated: the flag stays clear and no reduction applies.
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .remove(&id(0xB00));
    let weapon_only = game.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(!weapon_only.blocked);
    close(weapon_only.block_reduction, 0.);
    game.world_mut()
        .actor(id(0x901))
        .unwrap()
        .equipped
        .insert(id(0xB00));

    // Save/load continuation: the Block skill and the equipped shield are
    // saved state and come back unchanged. The live block stance is not
    // saved state, so the host re-signals it after load through
    // `World::set_blocking`, matching docs/combat.md.
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(extra);
    loaded.load_state(&saved).unwrap();
    close(
        loaded
            .world()
            .actor_ref(id(0x901))
            .unwrap()
            .values
            .get("block")
            .current(),
        50.,
    );
    let not_yet_reblocked = loaded.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(!not_yet_reblocked.blocked);
    close(not_yet_reblocked.block_reduction, 0.);
    loaded.world_mut().set_blocking(id(0x901), true).unwrap();
    let reblocked = loaded.melee_hit(hit(0x14, 0x901)).unwrap();
    assert!(reblocked.blocked);
    close(reblocked.block_reduction, 0.48);
    close(reblocked.health_damage, 7.8);
}
