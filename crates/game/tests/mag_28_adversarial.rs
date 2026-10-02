// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code, unused_imports)]

//! Spell execution through the world and Papyrus, using xEdit-shaped records.
//! CK lifecycle, recover, expiry and dispel contracts are asserted independently
//! of whether a spell remains learned or equipped.

#[allow(dead_code)]
use crate::common::legacy;

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

#[test]
fn mag_28_absorb_restores_caster_only_as_target_loses_the_authored_value() {
    let mut absorb = effect(1, 4, 25, false);
    let data = absorb
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"DATA"))
        .unwrap();
    put(&mut data.data, 88, 24);
    let mut game = game(plugin(absorb, vec![spell(SPELL, 0, false, 4.0, 2)]));
    game.world_mut()
        .damage_actor_value(id(OTHER), "magicka", 60.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(id(ACTOR), "magicka", 90.0)
        .unwrap();
    cast(&mut game, SPELL, OTHER).unwrap();
    game.tick(1.0);
    assert_eq!(game.world_mut().actor_value(id(OTHER), "magicka"), 36.0);
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "magicka"), 14.0);
}

#[test]
fn mag_28_absorb_elapsed_time_and_transfer_resume_after_save_load() {
    let mut absorb = effect(1, 4, 25, false);
    let data = absorb
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"DATA"))
        .unwrap();
    put(&mut data.data, 88, 24);
    let records = plugin(absorb, vec![spell(SPELL, 0, false, 4.0, 2)]);
    let mut original = game(records.clone());
    original
        .world_mut()
        .damage_actor_value(id(OTHER), "magicka", 60.0)
        .unwrap();
    original
        .world_mut()
        .damage_actor_value(id(ACTOR), "magicka", 90.0)
        .unwrap();
    cast(&mut original, SPELL, OTHER).unwrap();
    original.tick(1.0);
    let saved = original.save_state().unwrap();
    let mut restored = game(records);
    restored.load_state(&saved).unwrap();
    restored.tick(1.0);
    assert_eq!(restored.world_mut().actor_value(id(OTHER), "magicka"), 32.0);
    assert_eq!(restored.world_mut().actor_value(id(ACTOR), "magicka"), 18.0);
    assert_eq!(restored.world().active_effects().count(), 0);
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
