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
use soe_game::world::Handle;
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, CallMethod, IAdd, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const OTHER: u32 = 0x101;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const KEYWORD: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn effect(flags: u32, archetype: u32, av: u32, constant: bool) -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, flags);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, archetype);
    put(&mut data, 68, av);
    put(&mut data, 80, u32::from(!constant));
    put(&mut data, 84, 3);
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("EffectProbe")],
        ..Vmad::default()
    };
    Record::new(T(b"MGEF"), EFFECT)
        .zsub(T(b"EDID"), "ProbeEffect")
        .sub(T(b"DATA"), data)
        .sub(T(b"KWDA"), KEYWORD.to_le_bytes())
        .sub(T(b"VMAD"), vmad.write(T(b"MGEF")))
}

fn spell(form: u32, kind: u32, constant: bool, magnitude: f32, duration: u32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 8, kind);
    put(&mut data, 16, u32::from(!constant));
    put(&mut data, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&duration.to_le_bytes());
    Record::new(T(b"SPEL"), form)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

fn plugin(effect: Record, spells: Vec<Record>) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for actor in [ACTOR, OTHER] {
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500u32.to_le_bytes())
                .sub(T(b"DATA"), vec![0; 24]),
        );
    }
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(T(b"EDID"), "ProbeKeyword"));
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
    for actor in [ACTOR, OTHER] {
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
fn version_eight_worlds_load_without_active_effects() {
    let mut game = fixture(0, 1, 0.0, 10);
    let mut old =
        legacy::without_exclusive_branches(&mut game.world_mut(), &[id(ACTOR), id(OTHER)]);
    old.truncate(old.len() - 41);
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
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping magic survey");
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
    let spells: Vec<_> = order
        .winners_of(T(b"SPEL"))
        .filter(|(_, rec)| rec.flags & 0x20 == 0)
        .map(|(id, _)| id)
        .collect();
    assert!(
        spells.len() > 1000,
        "survey must cover the five-master spell corpus"
    );
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "health", 100.0)
        .unwrap();
    let baseline = game.world().save_state();
    let mut accepted = 0;
    let mut applications = 0;
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
                    assert!(game.world().active_effects().any(|effect| effect.id == id));
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
        "magic survey: {} spells, {accepted} accepted, {applications} applications",
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
}

#[test]
fn lethal_effect_receives_death_events_before_it_is_dispelled() {
    // Detrimental without hostile exercises the supported direct effect path.
    for duration in [0, 2] {
        let mut game = fixture(4, 0, 150.0, duration);
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
