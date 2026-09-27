// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Spell execution through the world and Papyrus, using xEdit-shaped records.
//! CK lifecycle, recover, expiry and dispel contracts are asserted independently
//! of whether a spell remains learned or equipped.

#[path = "common/legacy.rs"]
mod legacy;

use std::sync::atomic::{AtomicUsize, Ordering};

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
const LIGHT_BASE: u32 = 0x502;


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
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500u32.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
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


use soe_game::combat::Hand;
const BOUND_WEAPON: u32 = 0x700;
const RIGHT_SLOT: u32 = 0x701;
const LEFT_SLOT: u32 = 0x703;

fn bound_weapon_fixture() -> Game {
    let mut plugin = plugin(
        effect_with_associated_item(0, 17, 24, false, BOUND_WEAPON),
        vec![spell(SPELL, 0, false, 0.0, 10)],
    );
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&12_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 40, 0x2000);
    put(&mut dnam, 76, 6);
    put(&mut dnam, 88, u32::MAX);
    plugin.push_top(
        Record::new(T(b"WEAP"), BOUND_WEAPON)
            .sub(T(b"DATA"), data)
            .sub(T(b"DNAM"), dnam)
            .sub(T(b"ETYP"), RIGHT_SLOT.to_le_bytes()),
    );
    let mut slots = u32::from_le_bytes(*b"LHEQ").to_le_bytes().to_vec();
    slots.extend_from_slice(&LEFT_SLOT.to_le_bytes());
    slots.extend_from_slice(&u32::from_le_bytes(*b"RHEQ").to_le_bytes());
    slots.extend_from_slice(&RIGHT_SLOT.to_le_bytes());
    plugin.push_top(Record::new(T(b"DOBJ"), 0x702).sub(T(b"DNAM"), slots));
    plugin.push_top(Record::new(T(b"EQUP"), RIGHT_SLOT).sub(T(b"DATA"), 0_u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"EQUP"), LEFT_SLOT).sub(T(b"DATA"), 0_u32.to_le_bytes()));
    game(plugin)
}

#[test]
fn mag_38_bound_weapon_grants_and_equips_the_authored_item() {
    let mut game = bound_weapon_fixture();

    assert_eq!(cast(&mut game, SPELL, ACTOR).unwrap().len(), 1);
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);
    assert_eq!(
        game.world_mut().hand_weapon(id(ACTOR), Hand::Right),
        Some(id(BOUND_WEAPON))
    );
}

#[test]
fn mag_38_expiry_removes_the_effects_copy_and_clears_its_hand() {
    let mut game = bound_weapon_fixture();
    cast(&mut game, SPELL, ACTOR).unwrap();
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);

    game.world_mut()
        .tick_magic(10.0, &mut |_, _| soe_papyrus::ObjectId(999));

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 0);
    assert_eq!(game.world_mut().hand_weapon(id(ACTOR), Hand::Right), None);
    assert!(!game
        .world()
        .active_effects()
        .any(|effect| effect.effect == id(EFFECT)));
}

#[test]
fn mag_38_expiry_preserves_a_preexisting_weapon_copy_and_selection() {
    let mut game = bound_weapon_fixture();
    game.world_mut().add_item(id(ACTOR), id(BOUND_WEAPON), 1);
    cast(&mut game, SPELL, ACTOR).unwrap();
    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 2);

    game.world_mut()
        .tick_magic(10.0, &mut |_, _| soe_papyrus::ObjectId(999));

    assert_eq!(game.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);
    assert_eq!(
        game.world_mut().hand_weapon(id(ACTOR), Hand::Right),
        Some(id(BOUND_WEAPON))
    );
}

#[test]
fn mag_38_installed_bound_effects_resolve_to_weapon_records() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping MAG-38 installed bound weapon survey");
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
    let mut bound = 0;
    for (id, record) in order.winners_of(T(b"MGEF")) {
        if record.is_deleted() {
            continue;
        }
        let winner = order.winner_ref(id).unwrap();
        let localized = order.file(winner.file).plugin.header.is_localized();
        let effect =
            soe_esm::records::gameplay::MagicEffect::from_record(record, localized).unwrap();
        if effect.data.archetype != 17 {
            continue;
        }
        let weapon = order.resolve(winner.file, effect.data.associated_item);
        let weapon_record = order.winner_ref(weapon).unwrap();
        assert_eq!(
            order.record(weapon_record).tag,
            T(b"WEAP"),
            "Bound Weapon MGEF {id:?} must link to a WEAP record"
        );
        bound += 1;
    }
    assert!(
        bound > 0,
        "five-master load order should contain Bound Weapon effects"
    );
}

#[test]
fn mag_38_saved_bound_weapon_expires_without_regranting_or_leaving_equipment() {
    let records = bound_weapon_fixture();
    let bytes = {
        let mut original = records;
        cast(&mut original, SPELL, ACTOR).unwrap();
        original.save_state().unwrap()
    };
    let mut restored = bound_weapon_fixture();
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 1);
    restored.world_mut().tick_magic(10.0, &mut |_, _| soe_papyrus::ObjectId(999));
    assert_eq!(restored.world_mut().item_count(id(ACTOR), id(BOUND_WEAPON)), 0);
    assert_eq!(restored.world_mut().hand_weapon(id(ACTOR), Hand::Right), None);
}

fn cast(game: &mut Game, spell: u32, target: u32) -> Result<Vec<EffectId>, MagicError> {
    let shared = game.shared_world();
    let result = shared.borrow_mut().cast_spell(
        id(spell), id(SOURCE), Some(id(ACTOR)), Some(id(target)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    result
}
