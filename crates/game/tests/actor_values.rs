// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CK actor-value contracts through the native boundary, independent NPC
//! field bytes, and winning template/race inputs used by combat and magic.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    actor_values::Values,
    npc::{self, Config, Stats},
    Game,
};
use soe_papyrus::{
    asm::{f, ident, s, FunctionBuilder, ScriptBuilder},
    Opcode::CallMethod,
};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn fresh() -> (Game, GlobalFormId) {
    let mut game = Game::new(LoadOrder::new());
    let actor = game.player();
    (game, actor)
}

#[test]
fn player_without_a_plugin_reference_remains_an_actor_in_vm_and_form_lookup() {
    use soe_papyrus::{
        Opcode::{CallStatic, Return},
        Value,
    };
    let (mut game, actor) = fresh();
    let object = game.object_of_form(actor);
    assert_eq!(game.vm.object_class(object), Some("Actor"));
    let world = game.world();
    assert_eq!(world.runtime_id(actor), 0x14);
    assert_eq!(world.from_runtime_id(0x14), Some(actor));
    assert!(world.exists(actor));
    drop(world);
    game.load_script(
        &ScriptBuilder::new("PlayerLookup", "")
            .function(
                FunctionBuilder::new("RunLookup", "Actor")
                    .local("player", "Actor")
                    .ins(CallStatic, vec![s("Game"), s("GetPlayer"), ident("player")])
                    .ins(Return, vec![ident("player")])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    let probe = game.vm.create_object("probe");
    game.vm.attach(probe, "PlayerLookup").unwrap();
    let thread = game.vm.call(probe, "RunLookup", &[]).unwrap();
    game.run_until_idle(32);
    assert_eq!(game.vm.thread_result(thread), Some(Value::Object(object)));
}

fn native(
    game: &mut Game,
    actor: GlobalFormId,
    method: &str,
    name: &str,
    amount: Option<f32>,
) -> f32 {
    let class = format!("Probe{method}");
    let function = format!("Run{method}");
    let mut arguments = vec![s(method), ident("self"), ident("result"), s(name)];
    if let Some(value) = amount {
        arguments.push(f(value));
    }
    let script = ScriptBuilder::new(&class, "Actor")
        .variable("result", "Float", f(0.0))
        .function(
            FunctionBuilder::new(&function, "None")
                .ins(CallMethod, arguments)
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let object = game.object_of_form(actor);
    game.vm.attach(object, &class).unwrap();
    game.vm.call(object, &function, &[]).unwrap();
    game.run_until_idle(32);
    game.vm
        .get_var(object, &class, "result")
        .unwrap()
        .as_float()
}

#[test]
fn mod_and_damage_change_different_components_through_natives() {
    let (mut game, actor) = fresh();
    native(&mut game, actor, "ModAV", "health", Some(-10.0));
    native(&mut game, actor, "DamageAV", "health", Some(-20.0));
    assert_eq!(native(&mut game, actor, "GetAV", "health", None), 70.0);
    assert_eq!(native(&mut game, actor, "GetBaseAV", "health", None), 100.0);
    assert_eq!(native(&mut game, actor, "GetAVMax", "health", None), 90.0);
    assert!(
        (native(&mut game, actor, "GetAVPercentage", "health", None) - 70.0 / 90.0).abs() < 0.00001
    );
}

#[test]
fn set_base_preserves_damage_and_both_modifiers() {
    let (mut game, actor) = fresh();
    game.world_mut()
        .mod_actor_value(actor, "health", 20.0)
        .unwrap();
    game.world_mut()
        .mod_temporary_actor_value(actor, "health", 30.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(actor, "health", 40.0)
        .unwrap();
    native(&mut game, actor, "SetActorValue", "health", Some(200.0));
    assert_eq!(game.world_mut().actor_value(actor, "health"), 210.0);
    assert_eq!(game.world_mut().actor_value_maximum(actor, "health"), 250.0);
    assert_eq!(game.world_mut().base_actor_value(actor, "health"), 200.0);
}

#[test]
fn restore_heals_damage_and_never_undoes_a_permanent_debuff() {
    let (mut game, actor) = fresh();
    game.world_mut()
        .mod_actor_value(actor, "health", -25.0)
        .unwrap();
    game.world_mut()
        .damage_actor_value(actor, "health", 30.0)
        .unwrap();
    native(
        &mut game,
        actor,
        "RestoreActorValue",
        "health",
        Some(-500.0),
    );
    assert_eq!(game.world_mut().actor_value(actor, "health"), 75.0);
    assert_eq!(game.world_mut().actor_value_maximum(actor, "health"), 75.0);
    assert_eq!(game.world_mut().base_actor_value(actor, "health"), 100.0);
}

#[test]
fn force_preserves_damage_and_base_changes_keep_its_permanent_adjustment() {
    let (mut game, actor) = fresh();
    game.world_mut()
        .damage_actor_value(actor, "health", 20.0)
        .unwrap();
    native(&mut game, actor, "ForceAV", "health", Some(50.0));
    assert_eq!(game.world_mut().actor_value(actor, "health"), 50.0);
    game.world_mut()
        .set_actor_value(actor, "health", 125.0)
        .unwrap();
    assert_eq!(game.world_mut().actor_value(actor, "health"), 75.0);
    game.world_mut()
        .restore_actor_value(actor, "health", 20.0)
        .unwrap();
    assert_eq!(game.world_mut().actor_value(actor, "health"), 95.0);
}

#[test]
fn removing_a_temporary_buff_retains_damage_and_permanent_changes() {
    let mut values = Values::default();
    values.set_base("health", 100.0).unwrap();
    values.modify_temporary("health", 50.0).unwrap();
    values.damage("health", 30.0).unwrap();
    values.modify("health", 10.0).unwrap();
    values.modify_temporary("health", -50.0).unwrap();
    assert_eq!(values.get("health").current(), 80.0);
    assert_eq!(values.get("health").maximum(), 110.0);
}

#[test]
fn percentages_handle_zero_maximum_and_carry_weight() {
    let mut values = Values::default();
    assert_eq!(values.percentage("magicka"), 1.0);
    values.set_base("carryweight", 300.0).unwrap();
    values.damage("carryweight", 200.0).unwrap();
    assert_eq!(values.percentage("CarryWeight"), 1.0);
    values.set_base("health", 10.0).unwrap();
    values.damage("health", 20.0).unwrap();
    assert_eq!(values.percentage("health"), 0.0);
}

#[test]
fn invalid_mutations_preserve_existing_components() {
    let mut values = Values::default();
    values.set_base("health", 100.0).unwrap();
    let before = values.clone();
    for input in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(values.set_base("health", input).is_err());
        assert!(values.modify("health", input).is_err());
        assert!(values.modify_temporary("health", input).is_err());
        assert!(values.force("health", input).is_err());
        assert!(values.damage("health", input).is_err());
        assert!(values.restore("health", input).is_err());
        assert_eq!(values, before);
    }
    values.set_base("health", f32::MAX).unwrap();
    let before = values.clone();
    assert!(values.modify_temporary("health", f32::MAX).is_err());
    assert_eq!(values, before);
}

#[test]
fn save_preserves_components_that_have_the_same_current_total() {
    let (original, actor) = fresh();
    original
        .world_mut()
        .mod_actor_value(actor, "health", 25.0)
        .unwrap();
    original
        .world_mut()
        .mod_temporary_actor_value(actor, "health", 30.0)
        .unwrap();
    original
        .world_mut()
        .damage_actor_value(actor, "health", 55.0)
        .unwrap();
    let hash = original.world().state_hash();
    let bytes = original.save_state().unwrap();
    let (mut restored, _) = fresh();
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.world().state_hash(), hash);
    restored
        .world_mut()
        .restore_actor_value(actor, "health", 1000.0)
        .unwrap();
    restored
        .world_mut()
        .mod_temporary_actor_value(actor, "health", -30.0)
        .unwrap();
    assert_eq!(restored.world_mut().actor_value(actor, "health"), 125.0);
    assert_eq!(
        restored.world_mut().base_actor_value(actor, "health"),
        100.0
    );
}

#[test]
fn version_seven_components_load_without_dialogue_state() {
    let (original, actor) = fresh();
    original
        .world_mut()
        .damage_actor_value(actor, "health", 15.0)
        .unwrap();
    let hash = original.world().state_hash();
    let mut old = original.world().save_state();
    // Version 7 predates the empty dialogue cursor and history appended in v8.
    old.truncate(old.len() - 25);
    old[4..6].copy_from_slice(&7u16.to_le_bytes());
    let (restored, _) = fresh();
    restored.world_mut().load_state(&old).unwrap();
    assert_eq!(restored.world().state_hash(), hash);
    assert_eq!(restored.world_mut().actor_value(actor, "health"), 85.0);
    assert_eq!(
        restored.world_mut().actor_value_maximum(actor, "health"),
        100.0
    );
}

#[test]
fn health_damage_respects_essential_and_deferred_kill_flags() {
    let (game, actor) = fresh();
    let mut world = game.world_mut();
    world.actor(actor).unwrap().essential = true;
    world.damage_actor_value(actor, "health", 1000.0).unwrap();
    assert!(!world.actor(actor).unwrap().dead);
    world.restore_actor_value(actor, "health", 1000.0).unwrap();
    world.actor(actor).unwrap().essential = false;
    world.actor(actor).unwrap().deferred_kill = true;
    world.damage_actor_value(actor, "health", 1000.0).unwrap();
    assert!(!world.actor(actor).unwrap().dead);
    world.actor(actor).unwrap().deferred_kill = false;
    world.damage_actor_value(actor, "health", 1.0).unwrap();
    assert!(world.actor(actor).unwrap().dead);
}

#[test]
fn resurrection_clears_health_damage_and_retains_maximum_modifiers() {
    let (game, actor) = fresh();
    let mut world = game.world_mut();
    world.mod_actor_value(actor, "health", 25.0).unwrap();
    world
        .mod_temporary_actor_value(actor, "health", 30.0)
        .unwrap();
    assert!(world.kill_actor(actor, Some(actor)));
    assert_eq!(world.actor_value(actor, "health"), 0.0);
    world.resurrect_actor(actor).unwrap();
    assert_eq!(world.actor_value(actor, "health"), 155.0);
    assert_eq!(world.actor(actor).unwrap().killer, None);
    assert!(!world.actor(actor).unwrap().dead);
}

fn race(raw: u32, health: f32) -> Record {
    let mut bytes = vec![0u8; 128];
    for (offset, value) in [
        (36, health),
        (40, 50.0),
        (44, 60.0),
        (48, 300.0),
        (52, 1.0),
        (84, 0.7),
        (88, 3.0),
        (92, 5.0),
        (96, 4.0),
        (100, 64.0),
    ] {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    Record::new(T(b"RACE"), raw).sub(T(b"DATA"), bytes)
}

#[test]
fn dnam_fields_match_independent_schema_bytes() {
    let mut bytes = vec![0xABu8; 52];
    bytes[36..42].copy_from_slice(&[101, 0, 202, 0, 47, 1]);
    bytes[44..48].copy_from_slice(&123.5f32.to_le_bytes());
    bytes[48] = 3;
    let record = Record::new(T(b"NPC_"), 0x800).sub(T(b"DNAM"), bytes);
    let stats = npc::Npc::from_record(&record, false)
        .unwrap()
        .stats
        .unwrap();
    assert_eq!(
        (stats.health, stats.magicka, stats.stamina),
        (101, 202, 303)
    );
    assert_eq!(stats.far_away_distance, 123.5);
    assert_eq!(stats.geared_up_weapons, 3);
}

#[test]
fn race_starting_values_replace_non_autocalculated_dnam_cache() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 50.0));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Actor",
            "Actor",
            Config {
                health_offset: 50,
                magicka_offset: -20,
                stamina_offset: 40,
                ..Config::default()
            },
            Stats {
                health: 999,
                magicka: 999,
                stamina: 999,
                ..Stats::default()
            },
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(values[0].1, 100.0);
    assert_eq!(values[1].1, 30.0);
    assert_eq!(values[2].1, 100.0);
    assert!(values
        .iter()
        .any(|(name, value)| name == "unarmeddamage" && *value == 4.0));
}

#[test]
fn template_groups_follow_their_own_chains_and_winning_namespaces() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(race(0x700, 50.0));
    base.push_top(
        npc::build_record(
            0x800,
            "Root",
            "Root",
            Config {
                health_offset: 10,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(race(0x01000700, 80.0));
    patch.push_top(
        npc::build_record(
            0x800,
            "Root",
            "Root",
            Config {
                health_offset: 20,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x01000700u32.to_le_bytes()),
    );
    patch.push_top(
        npc::build_record(
            0x01000801,
            "Middle",
            "Middle",
            Config {
                template_flags: npc::template::STATS | npc::template::TRAITS,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"TPLT"), 0x800u32.to_le_bytes()),
    );
    let mut child = Plugin::new(FileHeader {
        masters: vec!["Actors.esm".into(), "Patch.esp".into()],
        ..FileHeader::default()
    });
    child.push_top(
        npc::build_record(
            0x02000802,
            "Child",
            "Child",
            Config {
                template_flags: npc::template::STATS,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"TPLT"), 0x01000801u32.to_le_bytes())
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Actors.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    order.add("Child.esp", child).unwrap();
    let game = Game::new(order);
    let world = game.world();
    assert_eq!(
        world.actor_base_race(id(1, 0x801)).unwrap(),
        Some(id(1, 0x700))
    );
    assert_eq!(
        world.initial_actor_values(id(1, 0x801)).unwrap()[0].1,
        100.0
    );
    assert_eq!(
        world.actor_base_race(id(2, 0x802)).unwrap(),
        Some(id(0, 0x700))
    );
    assert_eq!(world.initial_actor_values(id(2, 0x802)).unwrap()[0].1, 70.0);
}

#[test]
fn cyclic_template_sources_return_an_error() {
    let mut plugin = Plugin::new(FileHeader::default());
    for (raw, next) in [(0x800u32, 0x801u32), (0x801, 0x800)] {
        plugin.push_top(
            npc::build_record(
                raw,
                "",
                "",
                Config {
                    template_flags: npc::template::STATS,
                    ..Config::default()
                },
                Stats::default(),
            )
            .sub(T(b"TPLT"), next.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Cycle.esm", plugin).unwrap();
    let game = Game::new(order);
    assert!(game
        .world()
        .initial_actor_values(id(0, 0x800))
        .unwrap_err()
        .to_string()
        .contains("cyclic"));
}

#[test]
fn race_attributes_reject_short_and_nonfinite_data() {
    use soe_game::race::Attributes;
    assert_eq!(
        Attributes::from_record(&Record::new(T(b"RACE"), 1)).unwrap(),
        None
    );
    assert!(Attributes::from_record(&Record::new(T(b"NPC_"), 1)).is_err());
    assert!(
        Attributes::from_record(&Record::new(T(b"RACE"), 1).sub(T(b"DATA"), vec![0; 123])).is_err()
    );
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(Attributes::from_record(&race(1, bad)).is_err());
    }
}

#[test]
fn autocalculated_dnam_cache_is_not_given_the_offset_twice() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 50.0));
    plugin.push_top(
        npc::build_record(
            0x800,
            "Cached",
            "Cached",
            Config {
                flags: npc::flags::AUTO_CALC_STATS,
                health_offset: 50,
                magicka_offset: 20,
                stamina_offset: 40,
                ..Config::default()
            },
            Stats {
                health: 200,
                magicka: 70,
                stamina: 100,
                ..Stats::default()
            },
        )
        .sub(T(b"RNAM"), 0x700u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Cached.esm", plugin).unwrap();
    let game = Game::new(order);
    let values = game.world().initial_actor_values(id(0, 0x800)).unwrap();
    assert_eq!(
        [values[0].1, values[1].1, values[2].1],
        [200.0, 70.0, 100.0]
    );
}
