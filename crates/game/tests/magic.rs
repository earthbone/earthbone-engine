// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Spell execution through the world and Papyrus, using xEdit-shaped records.
//! CK lifecycle, recover, expiry and dispel contracts are asserted independently
//! of whether a spell remains learned or equipped.

#[path = "common/legacy.rs"]
mod legacy;

#[path = "common/installed.rs"]
mod installed;

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::records::navm::{Geometry, NavGrid, Parent, Triangle};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::world::magic::{EffectId, MagicError};
use soe_game::world::{Handle, Pending};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, CallMethod, IAdd, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const OTHER: u32 = 0x101;
const SOURCE: u32 = 0x102;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const POTION: u32 = 0x301;
const KEYWORD: u32 = 0x400;
const DISEASE_KEYWORD: u32 = 0x401;
const ITEM: u32 = 0x600;
const OTHER_ITEM: u32 = 0x601;
const SUMMON_BASE: u32 = 0x501;
const CELL: u32 = 0x800;
const NAVM: u32 = 0x810;
const LIGHT_BASE: u32 = 0x502;

#[path = "magic/absorb.rs"]
mod absorb;
#[path = "magic/bound_weapon.rs"]
mod bound_weapon;
#[path = "magic/calm.rs"]
mod calm;
#[path = "magic/collection.rs"]
mod collection;
#[path = "magic/disarm.rs"]
mod disarm;
#[path = "magic/invisibility.rs"]
mod invisibility;
#[path = "magic/mag_03_concentration.rs"]
mod mag_03_concentration;
#[path = "magic/mag_09_cast_cost.rs"]
mod mag_09_cast_cost;
#[path = "magic/mag_22_hand_cast.rs"]
mod mag_22_hand_cast;
#[path = "magic/mag_41_telekinesis.rs"]
mod mag_41_telekinesis;
#[path = "magic/mag_70_conditions.rs"]
mod mag_70_conditions;
#[path = "magic/scroll.rs"]
mod scroll;
#[path = "magic/worn_enchantment.rs"]
mod worn_enchantment;

fn retain_effect(game: &mut Game, effect: soe_papyrus::ObjectId) {
    let script = ScriptBuilder::new("EffectHolder", "Actor")
        .variable("held", "ActiveMagicEffect", soe_papyrus::asm::none())
        .bytes();
    game.load_script(&script).unwrap();
    let holder = game.object_of_form(id(ACTOR));
    game.vm.attach(holder, "EffectHolder").unwrap();
    game.vm
        .set_var(holder, "EffectHolder", "held", Value::Object(effect))
        .unwrap();
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn effect(flags: u32, archetype: u32, av: u32, constant: bool) -> Record {
    effect_with_associated_item(flags, archetype, av, constant, 0)
}

fn effect_with_associated_item(
    flags: u32,
    archetype: u32,
    av: u32,
    constant: bool,
    associated_item: u32,
) -> Record {
    effect_with_keywords(flags, archetype, av, constant, associated_item, &[KEYWORD])
}

fn effect_with_keywords(
    flags: u32,
    archetype: u32,
    av: u32,
    constant: bool,
    associated_item: u32,
    keywords: &[u32],
) -> Record {
    effect_with_id(
        EFFECT,
        flags,
        archetype,
        av,
        constant,
        associated_item,
        keywords,
    )
}

fn effect_with_id(
    form: u32,
    flags: u32,
    archetype: u32,
    av: u32,
    constant: bool,
    associated_item: u32,
    keywords: &[u32],
) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, flags);
    put(&mut data, 8, associated_item);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, av);
    put(&mut data, 80, u32::from(!constant));
    put(&mut data, 84, 3);
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("EffectProbe")],
        ..Vmad::default()
    };
    let keyword_data: Vec<u8> = keywords
        .iter()
        .flat_map(|keyword| keyword.to_le_bytes())
        .collect();
    Record::new(T(b"MGEF"), form)
        .zsub(T(b"EDID"), "ProbeEffect")
        .sub(T(b"DATA"), data)
        .sub(T(b"KWDA"), keyword_data)
        .sub(T(b"VMAD"), vmad.write(T(b"MGEF")))
}

fn effect_with_resistance(resistance: i32) -> Record {
    let mut effect = effect(1 | (1 << 2), 0, 24, false);
    effect.subrecords[1].data[16..20].copy_from_slice(&resistance.to_le_bytes());
    effect
}

fn location_effect() -> Record {
    let mut effect = effect(1 | (1 << 2), 0, 24, false);
    effect.subrecords[1].data[84..88].copy_from_slice(&4u32.to_le_bytes());
    effect
}

fn spell(form: u32, kind: u32, constant: bool, magnitude: f32, duration: u32) -> Record {
    spell_with_area(form, kind, constant, magnitude, duration, 0)
}

/// Like [`spell`], with the EFIT `Area` field set to `area` game units
/// instead of the point-effect default of 0.
fn spell_with_area(
    form: u32,
    kind: u32,
    constant: bool,
    magnitude: f32,
    duration: u32,
    area: u32,
) -> Record {
    spell_with_base_effect(form, kind, constant, magnitude, duration, area, EFFECT)
}

fn spell_with_base_effect(
    form: u32,
    kind: u32,
    constant: bool,
    magnitude: f32,
    duration: u32,
    area: u32,
    effect: u32,
) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, kind);
    put(&mut data, 16, u32::from(!constant));
    put(&mut data, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&area.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), effect.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn location_spell(form: u32, range: f32, area: u32) -> Record {
    let mut spell = spell_with_area(form, 0, false, 20.0, 0, area);
    put(&mut spell.subrecords[0].data, 20, 4);
    spell.subrecords[0].data[28..32].copy_from_slice(&range.to_le_bytes());
    spell
}

fn touch_spell(form: u32, magnitude: f32, reach: f32) -> Record {
    let mut spell = spell(form, 0, false, magnitude, 0);
    let spit = &mut spell.subrecords[0].data;
    spit[20..24].copy_from_slice(&1u32.to_le_bytes());
    spit[28..32].copy_from_slice(&reach.to_le_bytes());
    spell
}

fn touch_effect() -> Record {
    let mut effect = effect(5, 0, 24, false);
    effect.subrecords[1].data[84..88].copy_from_slice(&1u32.to_le_bytes());
    effect
}

fn aimed_spell(form: u32, magnitude: f32) -> Record {
    let mut spell = spell(form, 0, false, magnitude, 0);
    spell.subrecords[0].data[20..24].copy_from_slice(&2u32.to_le_bytes());
    spell
}

fn aimed_effect() -> Record {
    let mut effect = effect(5, 0, 24, false);
    effect.subrecords[1].data[84..88].copy_from_slice(&2u32.to_le_bytes());
    effect
}

fn condition(function: u16, parameter: u32, expected: f32, run_on: u32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data[20..24].copy_from_slice(&run_on.to_le_bytes());
    data
}

fn swapped_condition(function: u16, parameter: u32, expected: f32, run_on: u32) -> Vec<u8> {
    let mut data = condition(function, parameter, expected, run_on);
    data[0] = 0x10;
    data
}

fn ingestible(form: u32, magnitude: f32, duration: u32) -> Record {
    ingestible_with_flags(form, 0, magnitude, duration)
}

fn ingestible_with_flags(form: u32, flags: u32, magnitude: f32, duration: u32) -> Record {
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    let mut ingestible_data = vec![0; 20];
    put(&mut ingestible_data, 4, flags);
    Record::new(T(b"ALCH"), form)
        .sub(T(b"ENIT"), ingestible_data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn plugin(effect: Record, spells: Vec<Record>) -> Plugin {
    plugin_with_positions(effect, spells, &[])
}

fn plugin_with_positions(
    effect: Record,
    spells: Vec<Record>,
    positions: &[(u32, [f32; 3])],
) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    let mut summon_acbs = vec![0; 24];
    put(&mut summon_acbs, 0, 0x4000);
    plugin.push_top(
        Record::new(T(b"NPC_"), SUMMON_BASE)
            .zsub(T(b"EDID"), "SummonedActor")
            .sub(T(b"ACBS"), summon_acbs),
    );
    for actor in [ACTOR, OTHER, SOURCE] {
        let mut data = vec![0; 24];
        if let Some((_, position)) = positions.iter().find(|(form, _)| *form == actor) {
            for (index, value) in position.iter().enumerate() {
                let start = index * 4;
                data[start..start + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500u32.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    plugin.push_top(Record::new(T(b"ACTI"), 0x700));
    plugin.push_top(Record::new(T(b"REFR"), 0x701).sub(T(b"NAME"), 0x700u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(T(b"EDID"), "ProbeKeyword"));
    plugin.push_top(Record::new(T(b"MISC"), ITEM).zsub(T(b"EDID"), "AllowedItem"));
    plugin.push_top(Record::new(T(b"MISC"), OTHER_ITEM).zsub(T(b"EDID"), "RejectedItem"));
    plugin.push_top(effect);
    for spell in spells {
        plugin.push_top(spell);
    }
    plugin
}

fn game(plugin: Plugin) -> Game {
    let mut order = LoadOrder::new();
    order.add("Magic.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [ACTOR, OTHER, SOURCE] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut()
            .set_actor_value(id(actor), "magicka", 100.0)
            .unwrap();
    }
    let script = ScriptBuilder::new("EffectProbe", "ActiveMagicEffect")
        .variable("started", "Int", i(0))
        .variable("finished", "Int", i(0))
        .variable("added", "Int", i(0))
        .variable("removed", "Int", i(0))
        .variable("target", "Actor", soe_papyrus::asm::none())
        .variable("caster", "Actor", soe_papyrus::asm::none())
        .function(
            FunctionBuilder::new("OnEffectStart", "None")
                .param("akTarget", "Actor")
                .param("akCaster", "Actor")
                .ins(IAdd, vec![ident("started"), ident("started"), i(1)])
                .ins(Assign, vec![ident("target"), ident("akTarget")])
                .ins(Assign, vec![ident("caster"), ident("akCaster")])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnEffectFinish", "None")
                .param("akTarget", "Actor")
                .param("akCaster", "Actor")
                .ins(IAdd, vec![ident("finished"), ident("finished"), i(1)])
                .build(),
        )
        .function(
            FunctionBuilder::new("OnItemAdded", "None")
                .param("akBaseItem", "Form")
                .param("aiItemCount", "Int")
                .param("akItemReference", "ObjectReference")
                .param("akSourceContainer", "ObjectReference")
                .ins(
                    IAdd,
                    vec![ident("added"), ident("added"), ident("aiItemCount")],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnItemRemoved", "None")
                .param("akBaseItem", "Form")
                .param("aiItemCount", "Int")
                .param("akItemReference", "ObjectReference")
                .param("akDestContainer", "ObjectReference")
                .ins(
                    IAdd,
                    vec![ident("removed"), ident("removed"), ident("aiItemCount")],
                )
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    game.bind();
    game
}

fn fixture(flags: u32, archetype: u32, magnitude: f32, duration: u32) -> Game {
    game(plugin(
        effect(flags, archetype, 24, false),
        vec![spell(SPELL, 0, false, magnitude, duration)],
    ))
}

fn mag_30_navmesh() -> Geometry {
    Geometry {
        version: 12,
        location_crc: 0,
        parent: Parent::Cell(CELL),
        vertices: vec![
            [0.0, 0.0, 0.0],
            [1_000.0, 0.0, 0.0],
            [1_000.0, 1_000.0, 0.0],
            [0.0, 1_000.0, 0.0],
        ],
        triangles: vec![
            Triangle {
                vertices: [0, 1, 2],
                edges: [-1, -1, 1],
                flags: 0,
                cover_flags: 0,
            },
            Triangle {
                vertices: [0, 2, 3],
                edges: [0, -1, -1],
                flags: 0,
                cover_flags: 0,
            },
        ],
        edge_links: vec![],
        door_links: vec![],
        cover_triangles: vec![],
        grid: NavGrid {
            divisor: 1,
            size: [1_000.0, 1_000.0],
            min: [0.0, 0.0, 0.0],
            max: [1_000.0, 1_000.0, 0.0],
            cells: vec![vec![0, 1]],
        },
    }
}

#[test]
fn mag_11_drinking_an_ingestible_consumes_it_applies_its_effect_and_expires() {
    let mut game = game(plugin(
        effect(1 << 1, 0, 24, false),
        vec![ingestible(POTION, 20.0, 5)],
    ));
    let drinker = game.object_of_form(id(ACTOR));
    let effect = game.object_of_form(id(EFFECT));
    game.world_mut().add_item(id(ACTOR), id(POTION), 1);
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(ACTOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap();
    assert_eq!(applied.len(), 1);
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(POTION)), 0);
    assert!(game.world().pending.iter().any(|pending| {
        matches!(pending, Pending::Event { object, name, args }
            if *object == drinker && name == "OnMagicEffectApply"
                && args == &vec![Value::Object(drinker), Value::Object(effect)])
    }));
    let world = game.world();
    let active = world.active_effects().next().unwrap();
    assert_eq!(active.spell, id(POTION));
    assert_eq!(active.duration, Some(5.0));
    drop(world);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 120.0);
    game.tick(5.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_59_rally_modifies_confidence_for_its_duration_and_restores_prior_value() {
    let mut game = game(plugin(
        effect(0, 38, 24, false),
        vec![spell(SPELL, 0, false, 4.0, 3)],
    ));
    game.world_mut()
        .set_actor_value(id(ACTOR), "confidence", 2.0)
        .unwrap();

    let applied = cast(&mut game, SPELL, ACTOR).unwrap();
    assert_eq!(applied.len(), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 4.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);

    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 4.0);
    game.tick(2.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 2.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_59_rally_preserves_confidence_already_above_its_magnitude() {
    let mut game = game(plugin(
        effect(0, 38, 24, false),
        vec![spell(SPELL, 0, false, 3.0, 2)],
    ));
    game.world_mut()
        .set_actor_value(id(ACTOR), "confidence", 4.0)
        .unwrap();

    cast(&mut game, SPELL, ACTOR).unwrap();

    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 4.0);
    game.tick(2.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 4.0);
}

#[test]
fn mag_11_equip_item_drinks_a_potion_through_the_actor_native() {
    let mut game = game(plugin(
        effect(1 << 1, 0, 24, false),
        vec![ingestible(POTION, 20.0, 5)],
    ));
    let actor = Value::Object(game.object_of_form(id(ACTOR)));
    let potion = Value::Object(game.object_of_form(id(POTION)));
    native(
        &mut game,
        actor,
        "EquipItem",
        &[potion, Value::Bool(false), Value::Bool(true)],
    );
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(POTION)), 0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 120.0);
    assert_eq!(game.world().active_effects().count(), 1);
}

#[test]
fn mag_11_saved_timed_potion_keeps_its_effect_without_reconsuming_it() {
    let mut original = game(plugin(
        effect(1 << 1, 0, 24, false),
        vec![ingestible(POTION, 20.0, 5)],
    ));
    original.world_mut().add_item(id(ACTOR), id(POTION), 1);
    let shared = original.shared_world();
    shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(ACTOR), &mut |name, class| {
            original.vm.create_object_of(name, class)
        })
        .unwrap();
    let saved = original.save_state().unwrap();
    let mut restored = game(plugin(
        effect(1 << 1, 0, 24, false),
        vec![ingestible(POTION, 20.0, 5)],
    ));
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world_mut().item_count(id(ACTOR), id(POTION)), 0);
    assert_eq!(restored.world_mut().actor_value(id(ACTOR), "health"), 120.0);
    assert_eq!(restored.world().active_effects().count(), 1);
    restored.tick(5.0);
    assert_eq!(restored.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(restored.world().active_effects().count(), 0);
}

#[test]
fn mag_11_poison_waits_for_weapon_application_without_consuming_it() {
    let mut game = game(plugin(
        effect(1 << 1, 0, 24, false),
        vec![ingestible_with_flags(POTION, 1 << 17, 20.0, 5)],
    ));
    game.world_mut().add_item(id(ACTOR), id(POTION), 1);
    let shared = game.shared_world();
    let error = shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(ACTOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .unwrap_err();
    assert!(matches!(
        error,
        MagicError::Unsupported("poison application to a weapon")
    ));
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(POTION)), 1);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_12_magic_and_specific_resistance_apply_multiplicatively_in_order() {
    let mut game = game(plugin(
        effect_with_resistance(41),
        vec![spell(SPELL, 0, false, 100.0, 0)],
    ));
    game.world_mut()
        .set_actor_value(id(OTHER), "resistmagic", 50.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(OTHER), "resistfire", 50.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();

    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 75.0);
}

#[test]
fn mag_12_all_supported_specific_resistance_values_apply() {
    for (resistance, actor_value) in [
        (40, "poisonresist"),
        (41, "resistfire"),
        (42, "resistshock"),
        (43, "resistfrost"),
        (44, "resistmagic"),
        (45, "resistdisease"),
    ] {
        let mut game = game(plugin(
            effect_with_resistance(resistance),
            vec![spell(SPELL, 0, false, 100.0, 0)],
        ));
        game.world_mut()
            .set_actor_value(id(OTHER), actor_value, 50.0)
            .unwrap();

        cast(&mut game, SPELL, OTHER).unwrap();

        assert_eq!(
            game.world_mut().actor_value(id(OTHER), "health"),
            50.0,
            "MGEF resistance index {resistance} uses {actor_value}"
        );
    }
}

#[test]
fn mag_12_each_resistance_caps_at_eighty_five_percent() {
    let mut game = game(plugin(
        effect_with_resistance(41),
        vec![spell(SPELL, 0, false, 100.0, 0)],
    ));
    game.world_mut()
        .set_actor_value(id(OTHER), "resistmagic", 100.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(OTHER), "resistfire", 100.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();

    assert!((game.world_mut().actor_value(id(OTHER), "health") - 97.75).abs() < 0.001);
}

#[test]
fn mag_12_resistance_reads_independent_actor_value_modifiers() {
    let mut game = game(plugin(
        effect_with_resistance(42),
        vec![spell(SPELL, 0, false, 100.0, 0)],
    ));
    game.world_mut()
        .set_actor_value(id(OTHER), "resistmagic", 30.0)
        .unwrap();
    game.world_mut()
        .mod_actor_value(id(OTHER), "resistmagic", 20.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(OTHER), "resistshock", 40.0)
        .unwrap();
    game.world_mut()
        .mod_temporary_actor_value(id(OTHER), "resistshock", 10.0)
        .unwrap();

    cast(&mut game, SPELL, OTHER).unwrap();

    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 75.0);
}

#[test]
fn mag_11_malformed_effect_group_leaves_the_ingestible_in_inventory() {
    let potion = Record::new(T(b"ALCH"), POTION)
        .sub(T(b"ENIT"), vec![0; 20])
        .sub(T(b"EFID"), EFFECT.to_le_bytes());
    let mut game = game(plugin(effect(1 << 1, 0, 24, false), vec![potion]));
    game.world_mut().add_item(id(ACTOR), id(POTION), 1);
    let shared = game.shared_world();
    assert!(shared
        .borrow_mut()
        .drink_ingestible(id(POTION), id(ACTOR), &mut |name, class| {
            game.vm.create_object_of(name, class)
        })
        .is_err());
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(POTION)), 1);
    assert_eq!(game.world().active_effects().count(), 0);
}

fn cast(game: &mut Game, spell: u32, target: u32) -> Result<Vec<EffectId>, MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(spell),
        id(ACTOR),
        Some(id(ACTOR)),
        Some(id(target)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

fn projectile_contact(
    game: &mut Game,
    spell: u32,
    target: u32,
) -> Result<Vec<EffectId>, MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell_on_projectile_contact(
        id(spell),
        id(ACTOR),
        Some(id(ACTOR)),
        id(target),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

fn location_cast(
    game: &mut Game,
    spell: u32,
    position: [f32; 3],
    line_of_sight_clear: bool,
) -> Result<Vec<EffectId>, MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell_at_location(
        id(spell),
        id(ACTOR),
        Some(id(ACTOR)),
        position,
        line_of_sight_clear,
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}

#[test]
fn mag_02_aimed_projectile_contact_hits_only_its_hostile_actor_once() {
    let mut game = game(plugin(aimed_effect(), vec![aimed_spell(SPELL, 20.0)]));
    let target_object = game.object_of_form(id(OTHER));
    let caster_object = game.object_of_form(id(ACTOR));
    let spell_object = game.object_of_form(id(SPELL));
    game.world_mut().pending.clear();

    let applied = projectile_contact(&mut game, SPELL, OTHER).unwrap();

    assert_eq!(
        applied.len(),
        1,
        "one contact applies the one authored effect"
    );
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 80.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    let hits: Vec<_> = game
        .world()
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { object, name, args } if name == "OnHit" => {
                Some((*object, args.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        hits,
        vec![(
            target_object,
            vec![
                Value::Object(caster_object),
                Value::Object(spell_object),
                Value::None,
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false)
            ]
        )],
        "the contact emits one hostile hit for the resolved target"
    );
}

#[test]
fn mag_17_location_cast_uses_selected_position_and_authored_area() {
    let mut game = game(plugin_with_positions(
        location_effect(),
        vec![location_spell(SPELL, 100.0, 20)],
        &[
            (ACTOR, [0.0, 0.0, 0.0]),
            (OTHER, [105.0, 0.0, 0.0]),
            (SOURCE, [0.0, 500.0, 0.0]),
        ],
    ));

    let applied = location_cast(&mut game, SPELL, [100.0, 0.0, 0.0], true).unwrap();

    assert_eq!(applied.len(), 1);
    let world = game.world();
    let active = world.active_effects().next().unwrap();
    assert_eq!(active.target, id(OTHER));
    drop(world);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 80.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_a_point_beyond_authored_range() {
    let mut game = game(plugin_with_positions(
        location_effect(),
        vec![location_spell(SPELL, 50.0, 20)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [105.0, 0.0, 0.0])],
    ));

    assert!(matches!(
        location_cast(&mut game, SPELL, [100.0, 0.0, 0.0], true),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_an_obstructed_point() {
    let mut game = game(plugin_with_positions(
        location_effect(),
        vec![location_spell(SPELL, 200.0, 20)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [105.0, 0.0, 0.0])],
    ));

    assert!(matches!(
        location_cast(&mut game, SPELL, [100.0, 0.0, 0.0], false),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn mag_17_location_cast_rejects_non_finite_positions_and_range() {
    let mut position_game = game(plugin_with_positions(
        location_effect(),
        vec![location_spell(SPELL, 200.0, 20)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [105.0, 0.0, 0.0])],
    ));
    assert!(matches!(
        location_cast(&mut position_game, SPELL, [f32::NAN, 0.0, 0.0], true),
        Err(MagicError::Number)
    ));
    let mut malformed = location_spell(SPELL, 200.0, 20);
    malformed.subrecords[0].data[28..32].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut range_game = game(plugin_with_positions(
        location_effect(),
        vec![malformed],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [105.0, 0.0, 0.0])],
    ));
    assert!(matches!(
        location_cast(&mut range_game, SPELL, [100.0, 0.0, 0.0], true),
        Err(MagicError::Number)
    ));
    assert_eq!(range_game.world().active_effects().count(), 0);

    let mut negative_range_game = game(plugin_with_positions(
        location_effect(),
        vec![location_spell(SPELL, -1.0, 20)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [105.0, 0.0, 0.0])],
    ));
    assert!(matches!(
        location_cast(&mut negative_range_game, SPELL, [100.0, 0.0, 0.0], true),
        Err(MagicError::Number)
    ));
}

#[test]
fn mag_17_location_cast_rejects_a_non_location_spell() {
    let mut game = game(plugin_with_positions(
        effect(1, 0, 24, false),
        vec![spell(SPELL, 0, false, 20.0, 0)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [105.0, 0.0, 0.0])],
    ));

    assert!(matches!(
        location_cast(&mut game, SPELL, [100.0, 0.0, 0.0], true),
        Err(MagicError::Unsupported("location delivery"))
    ));
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_02_aimed_spell_requires_a_projectile_contact() {
    let mut game = game(plugin(aimed_effect(), vec![aimed_spell(SPELL, 20.0)]));

    let error = cast(&mut game, SPELL, OTHER).unwrap_err();

    assert!(matches!(error, MagicError::Unsupported(_)));
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_16_touch_spell_applies_at_its_authored_reach() {
    let game_plugin = plugin_with_positions(
        touch_effect(),
        vec![touch_spell(SPELL, 15.0, 10.0)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [10.0, 0.0, 0.0])],
    );
    let mut game = game(game_plugin);

    let applied = cast(&mut game, SPELL, OTHER).unwrap();

    assert_eq!(applied.len(), 1, "the target at the reach boundary is hit");
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 85.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}

#[test]
fn mag_16_touch_spell_rejects_a_target_beyond_its_authored_reach() {
    let game_plugin = plugin_with_positions(
        touch_effect(),
        vec![touch_spell(SPELL, 15.0, 10.0)],
        &[(ACTOR, [0.0, 0.0, 0.0]), (OTHER, [10.01, 0.0, 0.0])],
    );
    let mut game = game(game_plugin);

    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn mag_16_touch_spell_rejects_a_non_actor_target() {
    let mut game = game(plugin(touch_effect(), vec![touch_spell(SPELL, 15.0, 10.0)]));

    assert!(matches!(
        cast(&mut game, SPELL, ITEM),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_16_touch_hostile_spell_rejects_a_dead_target() {
    let mut game = game(plugin(touch_effect(), vec![touch_spell(SPELL, 15.0, 10.0)]));
    assert!(game
        .world_mut()
        .kill_actor_silent(id(OTHER), Some(id(ACTOR))));

    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Target)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_16_touch_spell_rejects_an_invalid_authored_reach() {
    let mut game = game(plugin(
        touch_effect(),
        vec![touch_spell(SPELL, 15.0, f32::NAN)],
    ));

    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Number)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn mag_39_summon_creature_spawns_associated_actor_owned_by_caster() {
    let summon = effect_with_associated_item(0, 18, 24, false, SUMMON_BASE);
    let mut game = game(plugin(summon, vec![spell(SPELL, 0, false, 0.0, 10)]));

    assert_eq!(cast(&mut game, SPELL, ACTOR).unwrap().len(), 1);
    let active = game.world().active_effects().next().unwrap().clone();
    let spawned = active
        .summoned
        .expect("summon effect must retain its actor");
    let world = game.world();
    let reference = world.reference_ref(spawned).unwrap();
    assert_eq!(reference.base, Some(id(SUMMON_BASE)));
    assert!(reference.is_actor);
    assert_eq!(reference.owner, Some(id(ACTOR)));
    assert!(reference.enabled);
}

#[test]
fn mag_35_authored_light_spawns_at_target_and_is_removed_on_expiry() {
    let light = effect_with_associated_item(0, 12, 24, false, LIGHT_BASE);
    let mut content = plugin(light, vec![spell(SPELL, 0, false, 0.0, 4)]);
    content.push_top(Record::new(T(b"LIGH"), LIGHT_BASE));
    let mut original = game(content.clone());
    let (position, rotation, cell) = {
        let mut world = original.world_mut();
        let reference = world.reference(id(OTHER)).unwrap();
        reference.position = [12.0, 34.0, 56.0];
        reference.rotation = [0.1, 0.2, 0.3];
        (reference.position, reference.rotation, reference.cell)
    };

    cast(&mut original, SPELL, OTHER).unwrap();
    let light_ref = original
        .world()
        .active_effects()
        .next()
        .unwrap()
        .summoned
        .unwrap();
    let world = original.world();
    let reference = world.reference_ref(light_ref).unwrap();
    assert_eq!(reference.base, Some(id(LIGHT_BASE)));
    assert_eq!(reference.position, position);
    assert_eq!(reference.rotation, rotation);
    assert_eq!(reference.cell, cell);
    assert!(reference.enabled);
    drop(world);

    let saved = original.save_state().unwrap();
    let mut restored = game(content);
    restored.load_state(&saved).unwrap();
    let light_ref = restored
        .world()
        .active_effects()
        .next()
        .unwrap()
        .summoned
        .unwrap();
    restored.tick(4.0);

    let world = restored.world();
    let reference = world.reference_ref(light_ref).unwrap();
    assert!(reference.deleted);
    assert!(!reference.enabled);
    assert_eq!(restored.world().active_effects().count(), 0);
}

#[test]
fn mag_39_summon_creature_dismisses_spawned_actor_on_expiry() {
    let summon = effect_with_associated_item(0, 18, 24, false, SUMMON_BASE);
    let mut game = game(plugin(summon, vec![spell(SPELL, 0, false, 0.0, 3)]));

    cast(&mut game, SPELL, ACTOR).unwrap();
    let spawned = game
        .world()
        .active_effects()
        .next()
        .unwrap()
        .summoned
        .unwrap();
    assert!(game.world().reference_ref(spawned).unwrap().enabled);

    game.tick(3.0);

    let world = game.world();
    let reference = world.reference_ref(spawned).unwrap();
    assert!(reference.deleted);
    assert!(!reference.enabled);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn mag_39_summon_check_spawns_authored_actor_with_caster_ownership_and_dismisses_on_expiry() {
    let summon = effect_with_associated_item(0, 18, 24, false, SUMMON_BASE);
    let mut game = game(plugin(summon, vec![spell(SPELL, 0, false, 0.0, 3)]));

    let effects = cast(&mut game, SPELL, ACTOR).unwrap();
    let active = game
        .world()
        .active_effects()
        .find(|effect| effect.id == effects[0])
        .unwrap()
        .clone();
    let spawned = active
        .summoned
        .expect("summon effect must retain its actor");
    {
        let world = game.world();
        let reference = world.reference_ref(spawned).unwrap();
        assert_eq!(reference.base, Some(id(SUMMON_BASE)));
        assert_eq!(reference.owner, Some(id(ACTOR)));
        assert!(reference.enabled);
    }

    game.tick(3.0);

    let world = game.world();
    let reference = world.reference_ref(spawned).unwrap();
    assert!(reference.deleted);
    assert!(!reference.enabled);
}

#[test]
fn mag_39_saved_summon_keeps_its_actor_link_until_expiry() {
    let summon = effect_with_associated_item(0, 18, 24, false, SUMMON_BASE);
    let plugin = plugin(summon, vec![spell(SPELL, 0, false, 0.0, 3)]);
    let mut original = game(plugin.clone());
    cast(&mut original, SPELL, ACTOR).unwrap();
    let spawned = original
        .world()
        .active_effects()
        .next()
        .unwrap()
        .summoned
        .unwrap();
    let saved = original.save_state().unwrap();

    let mut restored = game(plugin);
    restored.load_state(&saved).unwrap();
    assert_eq!(
        restored.world().active_effects().next().unwrap().summoned,
        Some(spawned)
    );
    restored.tick(3.0);

    let world = restored.world();
    let reference = world.reference_ref(spawned).unwrap();
    assert!(reference.deleted);
    assert!(!reference.enabled);
}

#[test]
fn magic_conditions_use_credited_caster_and_delivery_target() {
    let effect = effect(4, 0, 24, false)
        .sub(T(b"CTDA"), condition(136, ACTOR, 1.0, 0))
        .sub(T(b"CTDA"), condition(136, OTHER, 1.0, 1))
        .sub(T(b"CTDA"), swapped_condition(136, OTHER, 1.0, 0));
    let spell = spell(SPELL, 0, false, 10.0, 0).sub(T(b"CTDA"), condition(136, ACTOR, 1.0, 0));
    let mut game = game(plugin(effect, vec![spell]));
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(OTHER)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert_eq!(applied.len(), 1);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 90.0);

    let shared = game.shared_world();
    let skipped = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(OTHER)),
            Some(id(OTHER)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert!(skipped.is_empty());
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

/// mag_06: an area hit selects each eligible actor once using the spell
/// effect's authored radius, and an actor outside that radius takes no
/// effect. OTHER shares the explicit target's default position (distance
/// 0), SOURCE is moved outside the authored area.
#[test]
fn mag_06_area_hit_selects_each_eligible_actor_once_in_radius() {
    let effect = effect(1 | 4, 0, 24, false);
    let spell = spell_with_area(SPELL, 0, false, 15.0, 0, 500);
    let mut game = game(plugin(effect, vec![spell]));
    assert!(game
        .world_mut()
        .move_to(id(SOURCE), id(ACTOR), [2_000.0, 0.0, 0.0]));

    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    assert_eq!(
        applied.len(),
        2,
        "the explicit target and the bystander in radius are each selected once"
    );
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 85.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 85.0);
    assert_eq!(
        game.world_mut().actor_value(id(SOURCE), "health"),
        100.0,
        "an actor outside the authored radius takes no effect"
    );
}

/// mag_06: eligibility still applies inside the authored radius. A dead
/// actor is not a valid recipient of a hostile effect even when the blast
/// reaches it, so it is skipped while a living actor at the same distance
/// still takes the hit.
#[test]
fn mag_06_area_hit_skips_ineligible_actors_in_radius() {
    let effect = effect(1 | 4, 0, 24, false);
    let spell = spell_with_area(SPELL, 0, false, 15.0, 0, 500);
    let mut game = game(plugin(effect, vec![spell]));
    game.world_mut().actor(id(OTHER)).unwrap().dead = true;

    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    assert_eq!(
        applied.len(),
        2,
        "the dead bystander is skipped, the target and the living caster in radius are not"
    );
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 85.0);
    assert_eq!(
        game.world_mut().actor_value(id(OTHER), "health"),
        100.0,
        "a dead actor is not an eligible recipient of a hostile effect"
    );
    assert_eq!(game.world_mut().actor_value(id(SOURCE), "health"), 85.0);
}

#[test]
fn mag_30_demoralize_makes_eligible_actor_flee_and_lowers_confidence() {
    let mut plugin = plugin(
        effect(1 | 4, 7, 24, false),
        vec![spell(SPELL, 0, false, 5.0, 3)],
    );
    plugin.push_top(Record::new(T(b"CELL"), CELL));
    plugin.push_top(Record::new(T(b"NAVM"), NAVM).sub(T(b"NVNM"), mag_30_navmesh().to_bytes()));
    let mut game = game(plugin);
    assert!(game
        .world_mut()
        .move_to(id(ACTOR), id(SOURCE), [10.0, 10.0, 0.0]));
    game.world_mut()
        .set_actor_value(id(ACTOR), "confidence", 3.0)
        .unwrap();
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();

    assert_eq!(
        applied.len(),
        1,
        "the hostile effect applies to the live actor"
    );
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.in_combat = true;
        actor.combat_target = Some(id(OTHER));
    }
    let start = {
        let mut world = game.world_mut();
        world.reference(id(ACTOR)).unwrap().position
    };
    let start_distance = (start[0].powi(2) + start[1].powi(2)).sqrt();
    let shared = game.shared_world();
    shared.borrow_mut().ai_tick(0.1, &mut |name, class| {
        game.vm.create_object_of(name, class)
    });
    assert!(
        game.world().is_fleeing(id(ACTOR)),
        "the active effect overrides its AI"
    );
    let position = {
        let mut world = game.world_mut();
        world.reference(id(ACTOR)).unwrap().position
    };
    let distance = (position[0].powi(2) + position[1].powi(2)).sqrt();
    assert!(
        distance > start_distance,
        "the actor moved away from its caster"
    );
    assert!(!game.world().actor_ref(id(ACTOR)).unwrap().in_combat);
    assert_eq!(
        game.world().actor_ref(id(ACTOR)).unwrap().combat_target,
        None
    );
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), -2.0);
}

#[test]
fn mag_30_demoralize_rejects_dead_actor_without_applying_changes() {
    let mut game = fixture(1 | 4, 7, 5.0, 3);
    game.world_mut().actor(id(ACTOR)).unwrap().dead = true;
    game.world_mut()
        .set_actor_value(id(ACTOR), "confidence", 3.0)
        .unwrap();
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(SPELL),
        id(SOURCE),
        Some(id(SOURCE)),
        Some(id(ACTOR)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );

    assert!(
        matches!(result, Err(MagicError::Target)),
        "dead hostile targets are rejected"
    );
    assert!(
        !game.world().is_fleeing(id(ACTOR)),
        "a rejected actor does not flee"
    );
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 3.0);
}

#[test]
fn mag_30_demoralize_expiry_releases_flee_override_and_restores_confidence() {
    let mut game = fixture(1 | 4, 7, 5.0, 3);
    game.world_mut()
        .set_actor_value(id(ACTOR), "confidence", 3.0)
        .unwrap();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(SOURCE),
            Some(id(SOURCE)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert!(
        game.world().is_fleeing(id(ACTOR)),
        "the actor flees before expiry"
    );

    game.tick(3.0);

    assert!(
        !game.world().is_fleeing(id(ACTOR)),
        "expiry releases the AI override"
    );
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "confidence"), 3.0);
}

#[test]
fn spell_and_magic_effect_conditions_use_their_own_plugin_namespaces() {
    let effect = effect(4, 0, 24, false).sub(T(b"CTDA"), condition(74, 0x600, 1.0, 0));
    let mut first = plugin(effect, vec![]);
    first.push_top(Record::new(T(b"GLOB"), 0x600).sub(T(b"FLTV"), 1.0f32.to_le_bytes()));
    let mut second = Plugin::new(FileHeader::default());
    second.push_top(Record::new(T(b"GLOB"), 0x600).sub(T(b"FLTV"), 2.0f32.to_le_bytes()));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Second.esm".into(), "First.esm".into()],
        ..FileHeader::default()
    });
    let mut authored = spell(SPELL, 0, false, 10.0, 0);
    authored.form_id = 0x0200_0000 | SPELL;
    authored.subrecords[1].data = 0x0100_0200u32.to_le_bytes().to_vec();
    patch.push_top(
        authored
            .sub(T(b"EFID"), 0x0100_0200u32.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    10.0f32.to_le_bytes(),
                    0u32.to_le_bytes(),
                    0u32.to_le_bytes(),
                ]
                .concat(),
            )
            .sub(T(b"CTDA"), condition(74, 0x600, 2.0, 0)),
    );
    let mut order = LoadOrder::new();
    order.add("First.esm", first).unwrap();
    order.add("Second.esm", second).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut game = Game::new(order);
    for actor in [ACTOR, OTHER, SOURCE] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            GlobalFormId {
                file: 2,
                object: SPELL,
            },
            id(SOURCE),
            Some(id(OTHER)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert_eq!(applied.len(), 2);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 80.0);
}

#[test]
fn unsupported_later_magic_condition_leaves_earlier_effect_unapplied() {
    let mut second = 10.0f32.to_le_bytes().to_vec();
    second.extend_from_slice(&0u32.to_le_bytes());
    second.extend_from_slice(&0u32.to_le_bytes());
    let spell = spell(SPELL, 0, false, 10.0, 0)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), second)
        .sub(T(b"CTDA"), condition(u16::MAX, 0, 1.0, 0));
    let mut game = game(plugin(effect(4, 0, 24, false), vec![spell]));
    assert!(matches!(
        cast(&mut game, SPELL, ACTOR),
        Err(MagicError::Condition(_))
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert!(game.world().pending.is_empty());
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
}

fn dispel(game: &mut Game, effect: EffectId) -> bool {
    let shared = game.shared_world();
    let result = shared
        .borrow_mut()
        .dispel_effect(effect, &mut |name, class| {
            game.vm.create_object_of(name, class)
        });
    result
}

fn native(game: &mut Game, target: Value, name: &str, args: &[Value]) -> Value {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let script = format!("MagicCall{}", NEXT.fetch_add(1, Ordering::Relaxed));
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
    let bytes = ScriptBuilder::new(&script, "")
        .function(
            function
                .ins(CallMethod, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = vec![target];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn recover_instances_reverse_only_their_own_delta() {
    let mut game = fixture(2, 0, 20.0, 10);
    let first = cast(&mut game, SPELL, OTHER).unwrap()[0];
    let second = cast(&mut game, SPELL, OTHER).unwrap()[0];
    assert_ne!(first, second);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 140.0);
    game.world_mut()
        .mod_actor_value(id(OTHER), "health", -15.0)
        .unwrap();
    assert!(dispel(&mut game, first));
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 105.0);
    assert!(dispel(&mut game, second));
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 85.0);
}

#[test]
fn healing_integrates_final_partial_second_without_overhealing() {
    let mut game = fixture(0, 0, 12.0, 3);
    game.world_mut()
        .damage_actor_value(id(OTHER), "health", 50.0)
        .unwrap();
    cast(&mut game, SPELL, OTHER).unwrap();
    game.tick(2.5);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 80.0);
    game.tick(5.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 86.0);
    assert_eq!(game.world().active_effects().count(), 0);
    cast(&mut game, SPELL, OTHER).unwrap();
    game.tick(10.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn menu_time_does_not_age_effects() {
    let mut game = fixture(2, 0, 20.0, 2);
    cast(&mut game, SPELL, OTHER).unwrap();
    game.tick_menu_mode(20.0, true);
    assert_eq!(game.world().active_effects().next().unwrap().elapsed, 0.0);
    game.tick(2.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn scripts_bind_per_application_and_receive_target_and_caster() {
    let mut game = fixture(0, 1, 0.0, 10);
    let definition = game.object_of_form(id(EFFECT));
    assert!(game
        .vm
        .get_var(definition, "EffectProbe", "started")
        .is_none());
    let first = cast(&mut game, SPELL, ACTOR).unwrap()[0];
    let second = cast(&mut game, SPELL, OTHER).unwrap()[0];
    game.run_until_idle(64);
    let target = game.object_of_form(id(OTHER));
    let caster = game.object_of_form(id(ACTOR));
    let first_object = game.object_of(Handle::ActiveEffect(first));
    let second_object = game.object_of(Handle::ActiveEffect(second));
    retain_effect(&mut game, first_object);
    assert_eq!(
        game.vm
            .get_var(first_object, "EffectProbe", "target")
            .unwrap(),
        Value::Object(caster)
    );
    assert_eq!(
        game.vm
            .get_var(second_object, "EffectProbe", "target")
            .unwrap(),
        Value::Object(target)
    );
    assert_eq!(
        game.vm
            .get_var(second_object, "EffectProbe", "caster")
            .unwrap(),
        Value::Object(caster)
    );
    assert!(dispel(&mut game, first));
    game.run_until_idle(64);
    assert_eq!(
        game.vm
            .get_var(first_object, "EffectProbe", "finished")
            .unwrap(),
        Value::Int(1)
    );
    assert_eq!(
        game.vm
            .get_var(second_object, "EffectProbe", "finished")
            .unwrap(),
        Value::Int(0)
    );
}

#[test]
fn native_cast_queries_and_dispel_preserve_known_and_equipped_spells() {
    let mut game = fixture(2, 0, 20.0, 10);
    let source = Value::Object(game.object_of_form(id(ACTOR)));
    let target = Value::Object(game.object_of_form(id(OTHER)));
    let spell = Value::Object(game.object_of_form(id(SPELL)));
    let effect = Value::Object(game.object_of_form(id(EFFECT)));
    let keyword = Value::Object(game.object_of_form(id(KEYWORD)));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(OTHER)).unwrap();
        actor.spells.insert(id(SPELL));
        actor.equipped.insert(id(SPELL));
    }
    native(&mut game, spell.clone(), "Cast", &[source, target.clone()]);
    assert_eq!(
        native(&mut game, target.clone(), "HasMagicEffect", &[effect]),
        Value::Bool(true)
    );
    assert_eq!(
        native(
            &mut game,
            target.clone(),
            "HasMagicEffectWithKeyword",
            &[keyword]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        native(
            &mut game,
            target.clone(),
            "DispelSpell",
            std::slice::from_ref(&spell)
        ),
        Value::Bool(true)
    );
    assert_eq!(
        native(
            &mut game,
            target.clone(),
            "DispelSpell",
            std::slice::from_ref(&spell)
        ),
        Value::Bool(false)
    );
    assert_eq!(
        native(
            &mut game,
            target.clone(),
            "HasSpell",
            std::slice::from_ref(&spell)
        ),
        Value::Bool(true)
    );
    assert_eq!(
        native(&mut game, target, "IsEquipped", &[spell]),
        Value::Bool(true)
    );
}

#[test]
fn dispel_all_preserves_constant_abilities_diseases_and_addictions() {
    let spells = [0, 1, 4, 6, 10]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| spell(SPELL + index as u32, kind, true, 0.0, 0))
        .collect();
    let mut game = game(plugin(effect(0, 1, 24, true), spells));
    for index in 0..5 {
        cast(&mut game, SPELL + index, OTHER).unwrap();
    }
    let target = Value::Object(game.object_of_form(id(OTHER)));
    native(&mut game, target, "DispelAllSpells", &[]);
    let remaining: Vec<_> = game.world().active_effects().map(|e| e.spell).collect();
    assert_eq!(remaining, (1..5).map(|i| id(SPELL + i)).collect::<Vec<_>>());
}

#[test]
fn save_load_keeps_pending_bindings_and_does_not_reapply_modifiers() {
    let mut game = fixture(2, 0, 20.0, 5);
    let effect = cast(&mut game, SPELL, OTHER).unwrap()[0];
    let bytes = game.save_state().unwrap();
    let mut restored = fixture(2, 0, 20.0, 5);
    restored.load_state(&bytes).unwrap();
    assert_eq!(game.world().state_hash(), restored.world().state_hash());
    restored.run_until_idle(64);
    let object = restored.object_of(Handle::ActiveEffect(effect));
    retain_effect(&mut restored, object);
    assert_eq!(
        restored
            .vm
            .get_var(object, "EffectProbe", "started")
            .unwrap(),
        Value::Int(1)
    );
    assert_eq!(restored.world_mut().actor_value(id(OTHER), "health"), 120.0);
    restored.tick(5.0);
    assert_eq!(restored.world_mut().actor_value(id(OTHER), "health"), 100.0);
    assert_eq!(
        restored
            .vm
            .get_var(object, "EffectProbe", "finished")
            .unwrap(),
        Value::Int(1)
    );
}

#[test]
fn extended_actor_value_effect_survives_game_save_load_without_reapplying() {
    let mut original = game(plugin(
        effect(2, 0, 53, false),
        vec![spell(SPELL, 0, false, 20.0, 5)],
    ));
    cast(&mut original, SPELL, OTHER).unwrap();
    assert_eq!(
        original.world_mut().actor_value(id(OTHER), "paralysis"),
        20.0
    );

    let bytes = original.save_state().unwrap();
    let mut restored = game(plugin(
        effect(2, 0, 53, false),
        vec![spell(SPELL, 0, false, 20.0, 5)],
    ));
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.world_mut().actor_value(id(OTHER), "paralysis"),
        20.0
    );
    restored.tick(5.0);
    assert_eq!(
        restored.world_mut().actor_value(id(OTHER), "paralysis"),
        0.0
    );
}

#[test]
fn mag_27_cure_disease_removes_only_disease_effects_and_updates_keyword_queries() {
    const DISEASE_EFFECT: u32 = EFFECT + 1;
    const ORDINARY_EFFECT: u32 = EFFECT + 2;
    const CURE_EFFECT: u32 = EFFECT + 3;
    const DISEASE_SPELL: u32 = SPELL + 1;
    const ORDINARY_SPELL: u32 = SPELL + 2;
    const CURE_SPELL: u32 = SPELL + 3;

    let mut disease = effect_with_keywords(1 << 8, 0, 24, false, 0, &[KEYWORD, DISEASE_KEYWORD]);
    disease.form_id = DISEASE_EFFECT;
    let mut ordinary = effect(0, 0, 24, false);
    ordinary.form_id = ORDINARY_EFFECT;
    let mut cure = effect(0, 3, 24, false);
    cure.form_id = CURE_EFFECT;

    let mut plugin = plugin(effect(0, 0, 24, false), Vec::new());
    plugin.push_top(Record::new(T(b"KYWD"), DISEASE_KEYWORD).zsub(T(b"EDID"), "MagicDisease"));
    plugin.push_top(disease);
    plugin.push_top(ordinary);
    plugin.push_top(cure);
    for (form, effect) in [
        (DISEASE_SPELL, DISEASE_EFFECT),
        (ORDINARY_SPELL, ORDINARY_EFFECT),
        (CURE_SPELL, CURE_EFFECT),
    ] {
        plugin.push_top(spell_with_base_effect(form, 0, false, 0.0, 0, 0, effect));
    }
    let mut game = game(plugin);

    cast(&mut game, DISEASE_SPELL, OTHER).unwrap();
    cast(&mut game, ORDINARY_SPELL, OTHER).unwrap();
    assert!(game
        .world_mut()
        .has_magic_effect_with_keyword(id(OTHER), id(DISEASE_KEYWORD)));

    cast(&mut game, CURE_SPELL, OTHER).unwrap();

    assert!(!game
        .world_mut()
        .has_magic_effect_with_keyword(id(OTHER), id(DISEASE_KEYWORD)));
    assert!(!game.world().has_magic_effect(id(OTHER), id(DISEASE_EFFECT)));
    assert!(game
        .world()
        .has_magic_effect(id(OTHER), id(ORDINARY_EFFECT)));
    let remaining: std::collections::BTreeSet<_> = game
        .world()
        .active_effects()
        .map(|effect| effect.effect)
        .collect();
    assert_eq!(
        remaining,
        std::collections::BTreeSet::from([id(ORDINARY_EFFECT), id(CURE_EFFECT)])
    );
}

#[test]
fn mag_68_unnamed_archetypes_reject_without_changes() {
    for archetype in [13, 14] {
        let mut game = fixture(0, archetype, 20.0, 5);
        let before = game.save_state().unwrap();
        assert!(matches!(
            cast(&mut game, SPELL, OTHER),
            Err(MagicError::Unsupported("effect archetype"))
        ));
        assert_eq!(game.save_state().unwrap(), before);
    }
}

#[test]
fn mag_61_spawn_hazard_requires_owned_lifecycle_without_changes() {
    let mut game = fixture(0, 40, 20.0, 5);
    let before = game.save_state().unwrap();

    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Unsupported(
            "spawn hazard requires hazard ownership and expiry"
        ))
    ));
    assert_eq!(game.save_state().unwrap(), before);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn unsupported_second_effect_does_not_partially_apply_the_spell() {
    let good = effect(2, 0, 24, false);
    let mut bad = effect(0, 35, 24, false);
    bad.form_id = EFFECT + 1;
    let mut efit = 10.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&[0; 8]);
    let spell = spell(SPELL, 0, false, 20.0, 10)
        .sub(T(b"EFID"), (EFFECT + 1).to_le_bytes())
        .sub(T(b"EFIT"), efit);
    let mut plugin = plugin(good, vec![spell]);
    plugin.push_top(bad);
    let mut game = game(plugin);
    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Unsupported("effect archetype"))
    ));
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn no_recast_prevents_duplicate_application_until_dispelled() {
    let mut game = fixture(2 | (1 << 17), 0, 20.0, 10);
    let first = cast(&mut game, SPELL, OTHER).unwrap()[0];
    assert!(cast(&mut game, SPELL, OTHER).unwrap().is_empty());
    dispel(&mut game, first);
    assert_eq!(cast(&mut game, SPELL, OTHER).unwrap().len(), 1);
}

#[test]
fn zero_duration_healing_applies_once_and_finishes_on_next_tick() {
    let mut game = fixture(0, 0, 20.0, 0);
    game.world_mut()
        .damage_actor_value(id(OTHER), "health", 50.0)
        .unwrap();
    cast(&mut game, SPELL, OTHER).unwrap();
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 70.0);
    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 70.0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn active_effect_natives_distinguish_definition_target_and_blame_actor() {
    let mut game = fixture(0, 1, 0.0, 10);
    let source = Value::Object(game.object_of_form(id(ACTOR)));
    let target = Value::Object(game.object_of_form(id(OTHER)));
    let spell = Value::Object(game.object_of_form(id(SPELL)));
    let definition = Value::Object(game.object_of_form(id(EFFECT)));
    native(
        &mut game,
        spell,
        "RemoteCast",
        &[source, target.clone(), target.clone()],
    );
    let id = game.world().active_effects().next().unwrap().id;
    let effect = Value::Object(game.object_of(Handle::ActiveEffect(id)));
    assert_eq!(
        native(&mut game, effect.clone(), "GetBaseObject", &[]),
        definition
    );
    assert_eq!(
        native(&mut game, effect.clone(), "GetTargetActor", &[]),
        target
    );
    assert_eq!(
        native(&mut game, effect.clone(), "GetCasterActor", &[]),
        target
    );
    native(&mut game, effect.clone(), "Dispel", &[]);
    assert_eq!(native(&mut game, effect, "GetBaseObject", &[]), Value::None);
}

#[test]
fn active_effect_inventory_filters_forward_matching_events_until_expiry() {
    let mut game = fixture(0, 1, 0.0, 1);
    let application = cast(&mut game, SPELL, OTHER).unwrap()[0];
    assert!(game.run_until_idle(64));
    let effect = Value::Object(game.object_of(Handle::ActiveEffect(application)));
    let target = Value::Object(game.object_of_form(id(OTHER)));
    let allowed = Value::Object(game.object_of_form(id(ITEM)));
    let rejected = Value::Object(game.object_of_form(id(OTHER_ITEM)));
    native(
        &mut game,
        effect.clone(),
        "AddInventoryEventFilter",
        std::slice::from_ref(&allowed),
    );
    native(
        &mut game,
        target.clone(),
        "AddItem",
        &[allowed.clone(), Value::Int(2)],
    );
    native(
        &mut game,
        target.clone(),
        "RemoveItem",
        &[
            allowed.clone(),
            Value::Int(2),
            Value::Bool(true),
            Value::None,
        ],
    );
    native(
        &mut game,
        target.clone(),
        "AddItem",
        &[rejected.clone(), Value::Int(1)],
    );
    native(
        &mut game,
        target.clone(),
        "RemoveItem",
        &[rejected, Value::Int(1), Value::Bool(true), Value::None],
    );
    assert!(game.run_until_idle(64));
    assert_eq!(
        game.vm
            .get_var(effect.as_object().unwrap(), "EffectProbe", "added"),
        Some(Value::Int(2))
    );
    assert_eq!(
        game.vm
            .get_var(effect.as_object().unwrap(), "EffectProbe", "removed"),
        Some(Value::Int(2))
    );

    game.tick(1.0);
    assert!(game.run_until_idle(64));
    native(
        &mut game,
        target.clone(),
        "AddItem",
        &[allowed.clone(), Value::Int(1)],
    );
    native(
        &mut game,
        target,
        "RemoveItem",
        &[allowed, Value::Int(1), Value::Bool(true), Value::None],
    );
    assert!(game.run_until_idle(64));
    // The effect expired during the tick, so its script object is retired with
    // its filter. A filter that outlived its effect would answer here, either
    // with the old counts or with counts raised by the calls above.
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(
        game.vm
            .get_var(effect.as_object().unwrap(), "EffectProbe", "added"),
        None
    );
    assert_eq!(
        game.vm
            .get_var(effect.as_object().unwrap(), "EffectProbe", "removed"),
        None
    );
}

#[test]
fn version_eight_worlds_load_without_active_effects() {
    let mut game = fixture(0, 1, 0.0, 10);
    let actors = game.world().loaded_actors();
    let mut old = legacy::without_exclusive_branches(&mut game.world_mut(), &actors);
    // The extra 6 bytes beyond the old 70 are detection's now-unconditional
    // empty pool count (4 bytes) and the world-level save-section table
    // version 37 added (2 bytes), both always present at the tail regardless
    // of the simulated version. `without_exclusive_branches` already strips
    // each actor's and reference's own mid-stream section table.
    old.truncate(old.len() - 76);
    old[4..6].copy_from_slice(&8u16.to_le_bytes());
    cast(&mut game, SPELL, OTHER).unwrap();
    game.world_mut().load_state(&old).unwrap();
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn corrupt_effect_data_leaves_the_live_world_unchanged() {
    let mut game = fixture(2, 0, 20.0, 10);
    cast(&mut game, SPELL, OTHER).unwrap();
    let before = game.world().state_hash();
    let mut bytes = game.world().save_state();
    let offset = bytes.len() - 20;
    bytes[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    assert!(game.world_mut().load_state(&bytes).is_err());
    assert_eq!(game.world().state_hash(), before);
}

#[test]
fn overflowing_recover_cast_is_rejected_before_application() {
    let mut game = fixture(2, 0, f32::MAX, 10);
    game.world_mut()
        .set_actor_value(id(OTHER), "health", f32::MAX)
        .unwrap();
    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Number)
    ));
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn long_constant_effect_tick_keeps_values_and_saved_time_finite() {
    let mut game = game(plugin(
        effect(0, 0, 24, true),
        vec![spell(SPELL, 0, true, f32::MAX, 0)],
    ));
    cast(&mut game, SPELL, OTHER).unwrap();
    let shared = game.shared_world();
    for _ in 0..2 {
        shared
            .borrow_mut()
            .tick_magic(f64::MAX, &mut |name, class| {
                game.vm.create_object_of(name, class)
            });
    }
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
    let bytes = game.world().save_state();
    game.world_mut().load_state(&bytes).unwrap();
}

#[test]
fn installed_spell_survey_reports_supported_applications_without_running_scripts() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping magic survey");
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
    let spells: Vec<_> = order
        .winners_of(T(b"SPEL"))
        .filter(|(_, rec)| rec.flags & 0x20 == 0)
        .map(|(id, _)| id)
        .collect();
    assert!(
        spells.len() > 1000,
        "survey must cover the five-master spell corpus"
    );
    let mut game = Game::from_shared_order(order);
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "health", 100.0)
        .unwrap();
    let baseline = game.world().save_state();
    let mut accepted = 0;
    let mut applications = 0;
    let mut hostile_applications = 0;
    let mut rejected = std::collections::BTreeMap::new();
    let mut examples = Vec::new();
    for spell in &spells {
        game.world_mut().load_state(&baseline).unwrap();
        let shared = game.shared_world();
        let result = shared.borrow_mut().cast_spell(
            *spell,
            player,
            Some(player),
            Some(player),
            &mut |name, class| game.vm.create_object_of(name, class),
        );
        match result {
            Ok(ids) => {
                accepted += 1;
                applications += ids.len();
                if examples.len() < 15 {
                    examples.push(game.world().editor_id(*spell).unwrap_or_default());
                }
                for id in ids {
                    let effect = game
                        .world()
                        .active_effects()
                        .find(|effect| effect.id == id)
                        .unwrap()
                        .effect;
                    if game.world().magic_effect(effect).unwrap().data.flags & 1 != 0 {
                        hostile_applications += 1;
                    }
                    assert!(dispel(&mut game, id));
                }
                assert_eq!(game.world().active_effects().count(), 0);
            }
            Err(MagicError::Record(error)) => panic!("real record rejected {spell:?}: {error}"),
            Err(error) => {
                *rejected.entry(error.to_string()).or_insert(0usize) += 1;
                assert_eq!(game.world().active_effects().count(), 0);
            }
        }
    }
    eprintln!(
        "magic survey: {} spells, {accepted} accepted, {applications} applications, {hostile_applications} hostile",
        spells.len()
    );
    eprintln!("examples: {examples:?}");
    for (reason, count) in rejected {
        eprintln!("{count}: {reason}");
    }
    assert!(
        accepted > 0 && applications > 0,
        "at least one real spell must execute effects"
    );
    assert!(
        hostile_applications > 0,
        "at least one authored hostile effect must execute"
    );
}

#[test]
fn installed_ingestible_survey_reports_supported_applications() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping ingestible survey");
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
    let items: Vec<_> = order
        .winners_of(T(b"ALCH"))
        .filter(|(_, rec)| rec.flags & 0x20 == 0)
        .map(|(id, _)| id)
        .collect();
    assert!(
        items.len() > 100,
        "survey must cover the five-master ingestible corpus"
    );
    let mut game = Game::from_shared_order(order);
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "health", 100.0)
        .unwrap();
    let baseline = game.world().save_state();
    let mut accepted = 0;
    let mut applications = 0;
    let mut rejected = std::collections::BTreeMap::new();
    for item in items {
        game.world_mut().load_state(&baseline).unwrap();
        game.world_mut().add_item(player, item, 1);
        let shared = game.shared_world();
        let quantity = shared.borrow_mut().item_count(player, item);
        let before = shared.borrow().save_state();
        let result = shared
            .borrow_mut()
            .drink_ingestible(item, player, &mut |name, class| {
                game.vm.create_object_of(name, class)
            });
        match result {
            Ok(ids) => {
                accepted += 1;
                applications += ids.len();
                assert_eq!(shared.borrow_mut().item_count(player, item), quantity - 1);
            }
            Err(MagicError::Record(error)) => panic!("real ALCH record rejected {item:?}: {error}"),
            Err(error) => {
                assert_eq!(shared.borrow().save_state(), before, "rejected {item}");
                *rejected.entry(error.to_string()).or_insert(0usize) += 1;
            }
        }
    }
    eprintln!(
        "ingestible survey: {} records, {accepted} accepted, {applications} applications",
        game.world().order().winners_of(T(b"ALCH")).count()
    );
    for (reason, count) in rejected {
        eprintln!("{count}: {reason}");
    }
    assert!(
        accepted > 0 && applications > 0,
        "at least one real ingestible must execute effects"
    );
}

#[test]
fn lethal_effect_receives_death_events_before_it_is_dispelled() {
    for duration in [0, 2] {
        let mut game = fixture(5, 0, 150.0, duration);
        let observer = ScriptBuilder::new("EffectDeathProbe", "ActiveMagicEffect")
            .variable("sequence", "Int", i(0))
            .variable("killer", "Actor", soe_papyrus::asm::none())
            .function(
                FunctionBuilder::new("OnDying", "None")
                    .param("akKiller", "Actor")
                    .ins(Assign, vec![ident("sequence"), i(1)])
                    .ins(Assign, vec![ident("killer"), ident("akKiller")])
                    .build(),
            )
            .function(
                FunctionBuilder::new("OnDeath", "None")
                    .param("akKiller", "Actor")
                    .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(2)])
                    .build(),
            )
            .bytes();
        game.load_script(&observer).unwrap();
        let application = cast(&mut game, SPELL, OTHER).unwrap()[0];
        let object = game.object_of(Handle::ActiveEffect(application));
        retain_effect(&mut game, object);
        game.vm.attach(object, "EffectDeathProbe").unwrap();
        if duration == 0 {
            game.run_until_idle(32);
        } else {
            game.tick(1.0);
        }
        assert!(game.world().actor_ref(id(OTHER)).unwrap().dead);
        assert_eq!(
            game.vm.get_var(object, "EffectDeathProbe", "sequence"),
            Some(Value::Int(3))
        );
        let caster = game.object_of_form(id(ACTOR));
        assert_eq!(
            game.vm.get_var(object, "EffectDeathProbe", "killer"),
            Some(Value::Object(caster))
        );
        game.tick(0.0);
        assert_eq!(game.world().active_effects().count(), 0);
        assert_eq!(
            game.vm.get_var(object, "EffectProbe", "finished"),
            Some(Value::Int(1))
        );
        assert_eq!(
            game.vm.get_var(object, "EffectDeathProbe", "sequence"),
            Some(Value::Int(3))
        );
    }
}

#[test]
fn mag_01_hostile_hit_damages_only_the_target_with_spell_and_aggressor_arguments() {
    let mut game = fixture(5, 0, 20.0, 0);
    let target = game.object_of_form(id(OTHER));
    let caster = game.object_of_form(id(ACTOR));
    let spell = game.object_of_form(id(SPELL));
    game.world_mut().pending.clear();
    assert_eq!(cast(&mut game, SPELL, OTHER).unwrap().len(), 1);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 80.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    let world = game.world();
    let hits: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { object, name, args } if name == "OnHit" => {
                Some((*object, args.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        hits,
        vec![(
            target,
            vec![
                Value::Object(caster),
                Value::Object(spell),
                Value::None,
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false),
                Value::Bool(false)
            ]
        )]
    );
}

#[test]
fn hostile_no_hit_event_applies_damage_without_hit_callbacks() {
    let mut game = fixture(5 | (1 << 4), 0, 20.0, 0);
    game.world_mut().pending.clear();
    cast(&mut game, SPELL, OTHER).unwrap();
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 80.0);
    assert!(!game
        .world()
        .pending
        .iter()
        .any(|p| matches!(p, Pending::Event { name, .. } if name == "OnHit")));
}

#[test]
fn hostile_no_recast_emits_no_second_hit() {
    let mut game = fixture(5 | (1 << 17), 0, 20.0, 5);
    cast(&mut game, SPELL, OTHER).unwrap();
    game.world_mut().pending.clear();
    assert!(cast(&mut game, SPELL, OTHER).unwrap().is_empty());
    assert!(game.world().pending.is_empty());
}

#[test]
fn hostile_hits_reject_dead_ghost_and_missing_targets_without_events_or_damage() {
    for mode in 0..3 {
        let mut game = fixture(5, 0, 20.0, 0);
        let target = if mode == 2 { 0xBAD } else { OTHER };
        if mode == 0 {
            game.world_mut().actor(id(OTHER)).unwrap().dead = true;
        }
        if mode == 1 {
            game.world_mut().actor(id(OTHER)).unwrap().ghost = true;
        }
        game.world_mut().pending.clear();
        assert!(matches!(
            cast(&mut game, SPELL, target),
            Err(MagicError::Target)
        ));
        assert!(game.world().pending.is_empty());
        assert_eq!(game.world().active_effects().count(), 0);
        assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
    }
}

#[test]
fn invalid_second_hostile_effect_rejects_before_first_hit() {
    let good = effect(5, 0, 24, false);
    let mut bad = effect(5, 35, 24, false);
    bad.form_id = EFFECT + 1;
    let mut efit = 10.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&[0; 8]);
    let spell = spell(SPELL, 0, false, 20.0, 0)
        .sub(T(b"EFID"), (EFFECT + 1).to_le_bytes())
        .sub(T(b"EFIT"), efit);
    let mut plugin = plugin(good, vec![spell]);
    plugin.push_top(bad);
    let mut game = game(plugin);
    game.world_mut().pending.clear();
    assert!(matches!(
        cast(&mut game, SPELL, OTHER),
        Err(MagicError::Unsupported("effect archetype"))
    ));
    assert!(game.world().pending.is_empty());
    assert_eq!(game.world_mut().actor_value(id(OTHER), "health"), 100.0);
}

#[test]
fn remote_hostile_hit_credits_blame_actor_before_lethal_damage() {
    let mut game = fixture(5, 0, 150.0, 0);
    let source = game.object_of_form(id(ACTOR));
    let blame = game.object_of_form(id(OTHER));
    game.world_mut().pending.clear();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(ACTOR),
            Some(id(OTHER)),
            Some(id(ACTOR)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    let world = game.world();
    let hit = world
        .pending
        .iter()
        .position(|p| {
            matches!(p, Pending::Event { object, name, args }
        if *object == source && name == "OnHit" && args[0] == Value::Object(blame))
        })
        .unwrap();
    let death = world
        .pending
        .iter()
        .position(|p| {
            matches!(p, Pending::ActorDeath { actor, killer, .. }
        if *actor == id(ACTOR) && *killer == Some(id(OTHER)))
        })
        .unwrap();
    assert!(hit < death);
}

#[test]
fn each_hostile_effect_emits_a_hit_to_existing_effect_and_alias_listeners() {
    let mut efit = 10.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&5u32.to_le_bytes());
    let spell = spell(SPELL, 0, false, 20.0, 5)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit);
    let mut plugin = plugin(effect(5, 0, 24, false), vec![spell]);
    plugin.push_top(Record::new(T(b"QUST"), 0x900));
    let mut game = game(plugin);
    let existing = cast(&mut game, SPELL, OTHER).unwrap()[0];
    let actor = game.object_of_form(id(OTHER));
    let effect = game.object_of(Handle::ActiveEffect(existing));
    let alias = game.object_of(Handle::Alias {
        quest: id(0x900),
        id: 0,
    });
    let mut world = game.world_mut();
    let quest = world.quest_state(id(0x900)).unwrap();
    quest.running = true;
    quest.aliases.insert(0, Some(id(OTHER)));
    world.pending.clear();
    drop(world);
    cast(&mut game, SPELL, OTHER).unwrap();
    for recipient in [actor, effect, alias] {
        assert_eq!(
            game.world()
                .pending
                .iter()
                .filter(|p| matches!(p, Pending::Event { object, name, .. }
            if *object == recipient && name == "OnHit"))
                .count(),
            2
        );
    }
}

#[path = "magic/dispel.rs"]
mod dispel_archetype;
