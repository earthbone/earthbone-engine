// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CK death/deferred-kill behavior through shared state, Papyrus and engine saves.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game, Handle, Pending};
use soe_papyrus::{
    asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder},
    ObjectId,
    Opcode::*,
    Value,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fresh() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), object).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(quest::build_record(0xA00, "DeathQuest", 0));
    let mut order = LoadOrder::new();
    order.add("Death.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x900, 0x901, 0x902] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

fn watch(game: &mut Game, handle: Handle, class: &str, parent: &str) -> ObjectId {
    let mut script = ScriptBuilder::new(class, parent)
        .variable("sequence", "Int", i(0))
        .variable("killer", "Actor", none());
    for (name, digit) in [("OnDying", 1), ("OnDeath", 2)] {
        script = script.function(
            FunctionBuilder::new(name, "None")
                .param("akKiller", "Actor")
                .local("next", "Int")
                .ins(IMul, vec![ident("next"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("next"), i(digit)])
                .ins(Assign, vec![ident("killer"), ident("akKiller")])
                .build(),
        );
    }
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of(handle);
    game.vm.attach(object, class).unwrap();
    object
}

fn sequence(game: &Game, object: ObjectId, class: &str) -> i32 {
    game.vm.get_var(object, class, "sequence").unwrap().as_int()
}

fn native(game: &mut Game, actor: GlobalFormId, method: &str, args: &[Value]) {
    let class = format!("Invoke{method}");
    let mut code = vec![s(method), ident("self"), ident("result")];
    let mut function =
        FunctionBuilder::new(&format!("Run{method}"), "None").local("result", "None");
    for index in 0..args.len() {
        let name = format!("arg{index}");
        function = function.param(&name, "Var");
        code.push(ident(&name));
    }
    game.load_script(
        &ScriptBuilder::new(&class, "Actor")
            .function(function.ins(CallMethod, code).build())
            .bytes(),
    )
    .unwrap();
    let object = game.object_of_form(actor);
    game.vm.attach(object, &class).unwrap();
    let thread = game.vm.call(object, &format!("Run{method}"), args).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

#[test]
fn deferred_kill_survives_healing_and_engine_save_without_busy_pumping() {
    let mut game = fresh();
    let watcher = watch(&mut game, Handle::Form(id(0x900)), "DeathObserver", "Actor");
    let killer = game.object_of_form(id(0x901));
    native(&mut game, id(0x900), "StartDeferredKill", &[]);
    native(&mut game, id(0x900), "Kill", &[Value::Object(killer)]);
    assert!(!game.world().actor_ref(id(0x900)).unwrap().dead);
    assert_eq!(game.world_mut().actor_value(id(0x900), "health"), 0.0);
    game.world_mut()
        .restore_actor_value(id(0x900), "health", 100.0)
        .unwrap();
    assert_eq!(game.world_mut().actor_value(id(0x900), "health"), 100.0);
    assert_eq!(game.pump(), 0);
    assert!(game.run_until_idle(1));
    assert_eq!(sequence(&game, watcher, "DeathObserver"), 0);
    let bytes = game.save_state().unwrap();
    let mut loaded = fresh();
    watch(
        &mut loaded,
        Handle::Form(id(0x900)),
        "DeathObserver",
        "Actor",
    );
    // Saved native probe scripts must be available to restore their instances.
    native(&mut loaded, id(0x902), "StartDeferredKill", &[]);
    native(&mut loaded, id(0x902), "Kill", &[Value::None]);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.world().state_hash(), game.world().state_hash());
    native(&mut loaded, id(0x900), "EndDeferredKill", &[]);
    let world = loaded.world();
    let actor = world.actor_ref(id(0x900)).unwrap();
    assert!(actor.dead);
    assert_eq!(actor.killer, Some(id(0x901)));
    assert!(!actor.deferred_kill);
    drop(world);
    assert_eq!(sequence(&loaded, watcher, "DeathObserver"), 12);
    assert_eq!(
        loaded.vm.get_var(watcher, "DeathObserver", "killer"),
        Some(Value::Object(killer))
    );
}

#[test]
fn ordinary_death_is_idempotent_and_alias_recipients_are_captured_before_clear() {
    let mut game = fresh();
    let actor = watch(&mut game, Handle::Form(id(0x900)), "ActorObserver", "Actor");
    let alias = watch(
        &mut game,
        Handle::Alias {
            quest: id(0xA00),
            id: 3,
        },
        "AliasObserver",
        "ReferenceAlias",
    );
    game.world_mut().quest_state(id(0xA00)).unwrap().running = true;
    game.world_mut()
        .quest_state(id(0xA00))
        .unwrap()
        .aliases
        .insert(3, Some(id(0x900)));
    assert!(game.world_mut().kill_actor(id(0x900), Some(id(0x901))));
    assert!(!game.world_mut().kill_actor(id(0x900), Some(id(0x902))));
    game.world_mut()
        .quest_state(id(0xA00))
        .unwrap()
        .aliases
        .insert(3, None);
    assert_eq!(
        game.world().actor_ref(id(0x900)).unwrap().killer,
        Some(id(0x901))
    );
    assert!(game.run_until_idle(32));
    assert_eq!(sequence(&game, actor, "ActorObserver"), 12);
    assert_eq!(sequence(&game, alias, "AliasObserver"), 12);
    assert!(game.run_until_idle(1));
    assert_eq!(sequence(&game, actor, "ActorObserver"), 12);
}

#[test]
fn deferred_damage_latches_death_and_resurrection_cancels_the_held_request() {
    let game = fresh();
    let mut world = game.world_mut();
    world.start_deferred_kill(id(0x900));
    world
        .damage_actor_value(id(0x900), "health", 150.0)
        .unwrap();
    world
        .restore_actor_value(id(0x900), "health", 150.0)
        .unwrap();
    assert!(world.end_deferred_kill(id(0x900)));
    assert!(!world.end_deferred_kill(id(0x900)));
    world.start_deferred_kill(id(0x901));
    world.kill_actor(id(0x901), None);
    world.resurrect_actor(id(0x901)).unwrap();
    assert!(!world.end_deferred_kill(id(0x901)));
    assert!(!world.actor_ref(id(0x901)).unwrap().dead);
}

#[test]
fn essential_base_edits_affect_existing_and_later_actors() {
    let game = fresh();
    let mut world = game.world_mut();
    world.actor(id(0x900)).unwrap();
    world.set_actor_base_essential(id(0x800), true);
    assert!(world.actor_ref(id(0x900)).unwrap().essential);
    assert!(world.actor(id(0x901)).unwrap().essential);
    assert!(!world.kill_actor(id(0x900), None));
    assert!(!world.kill_actor(id(0x901), None));
    world.set_actor_base_essential(id(0x800), false);
    assert!(world.kill_actor(id(0x900), None));
    assert!(world.kill_actor(id(0x901), None));
}

#[test]
fn silent_death_stays_silent_when_deferred_and_reloaded() {
    let mut game = fresh();
    let actor = watch(
        &mut game,
        Handle::Form(id(0x900)),
        "SilentObserver",
        "Actor",
    );
    game.world_mut().start_deferred_kill(id(0x900));
    assert!(!game
        .world_mut()
        .kill_actor_silent(id(0x900), Some(id(0x901))));
    let bytes = game.world().save_state();
    game.world_mut().load_state(&bytes).unwrap();
    assert!(game.world_mut().end_deferred_kill(id(0x900)));
    assert!(game.run_until_idle(32));
    assert_eq!(sequence(&game, actor, "SilentObserver"), 0);
    assert_eq!(
        game.world().actor_ref(id(0x900)).unwrap().killer,
        Some(id(0x901))
    );
}

#[test]
fn malformed_pending_deaths_are_rejected_without_changing_the_world() {
    for event in [
        Pending::DeferredDeath {
            actor: id(0x900),
            killer: None,
            silent: false,
        },
        Pending::ActorDeath {
            actor: id(0x900),
            killer: None,
            recipients: vec![],
        },
        Pending::ActorDeath {
            actor: id(0x900),
            killer: None,
            recipients: vec![Handle::Form(id(0x900)); 2],
        },
    ] {
        let game = fresh();
        game.world_mut().actor(id(0x900)).unwrap();
        let before = game.world().save_state();
        game.world_mut().pending.push(event);
        let bad = game.world().save_state();
        game.world_mut().load_state(&before).unwrap();
        assert!(game.world_mut().load_state(&bad).is_err());
        assert_eq!(game.world().save_state(), before);
    }
}

#[test]
fn lethal_damage_native_and_queued_death_survive_game_save_once() {
    let mut game = fresh();
    let observer = watch(
        &mut game,
        Handle::Form(id(0x900)),
        "DamageObserver",
        "Actor",
    );
    native(
        &mut game,
        id(0x900),
        "DamageActorValue",
        &[Value::string("health"), Value::Float(101.0)],
    );
    assert!(game.world().actor_ref(id(0x900)).unwrap().dead);
    assert_eq!(sequence(&game, observer, "DamageObserver"), 12);
    assert_eq!(
        game.vm.get_var(observer, "DamageObserver", "killer"),
        Some(Value::None)
    );

    let observer = watch(
        &mut game,
        Handle::Form(id(0x901)),
        "QueuedObserver",
        "Actor",
    );
    game.world_mut().kill_actor(id(0x901), Some(id(0x902)));
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    assert_eq!(sequence(&game, observer, "QueuedObserver"), 0);
    assert!(game.run_until_idle(32));
    assert_eq!(sequence(&game, observer, "QueuedObserver"), 12);
    let bytes = game.save_state().unwrap();
    game.load_state(&bytes).unwrap();
    assert!(game.run_until_idle(1));
    assert_eq!(sequence(&game, observer, "QueuedObserver"), 12);
}

#[test]
fn silent_native_retains_culprit_without_death_notifications() {
    let mut game = fresh();
    let observer = watch(
        &mut game,
        Handle::Form(id(0x900)),
        "SilentNativeObserver",
        "Actor",
    );
    let culprit = game.object_of_form(id(0x901));
    native(
        &mut game,
        id(0x900),
        "KillSilent",
        &[Value::Object(culprit)],
    );
    assert!(game.world().actor_ref(id(0x900)).unwrap().dead);
    assert_eq!(
        game.world().actor_ref(id(0x900)).unwrap().killer,
        Some(id(0x901))
    );
    assert_eq!(sequence(&game, observer, "SilentNativeObserver"), 0);
}

#[test]
fn old_saves_resolve_deferred_health_but_reject_new_queue_tags() {
    let game = fresh();
    game.world_mut().start_deferred_kill(id(0x900));
    game.world_mut().kill_actor(id(0x900), None);
    let mut bytes = game.world().save_state();
    bytes.truncate(bytes.len() - 4);
    bytes[4..6].copy_from_slice(&14_u16.to_le_bytes());
    let before = game.world().save_state();
    assert!(game.world_mut().load_state(&bytes).is_err());
    assert_eq!(game.world().save_state(), before);
    // Version 14 held only the boolean and health, without a queued request.
    game.world_mut().pending.clear();
    let mut bytes = game.world().save_state();
    bytes.truncate(bytes.len() - 5);
    bytes[4..6].copy_from_slice(&14_u16.to_le_bytes());
    game.world_mut().load_state(&bytes).unwrap();
    assert!(game.world_mut().end_deferred_kill(id(0x900)));
    assert!(game.world().actor_ref(id(0x900)).unwrap().dead);
}

#[test]
fn duplicate_held_requests_are_rejected_atomically() {
    let game = fresh();
    game.world_mut().start_deferred_kill(id(0x900));
    game.world_mut().kill_actor(id(0x900), None);
    let before = game.world().save_state();
    game.world_mut().pending.push(Pending::DeferredDeath {
        actor: id(0x900),
        killer: None,
        silent: false,
    });
    let bad = game.world().save_state();
    game.world_mut().load_state(&before).unwrap();
    assert!(game.world_mut().load_state(&bad).is_err());
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn installed_npc_deaths_respect_essential_flags_and_persist() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping death lifecycle survey");
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
    let bases: Vec<_> = order
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    assert!(bases.len() > 6000, "expected all five masters");
    let mut world = soe_game::World::new(order);
    let player = world.player();
    let mut initialized = 0;
    let mut killed = 0;
    let mut essential = 0;
    let mut suppressed = 0;
    for (index, base) in bases.into_iter().enumerate() {
        let actor = world.place_at(base, player, false);
        if !world.prepare_actor_template(actor, base, 1).unwrap() {
            suppressed += 1;
            continue;
        }
        let protected = world.actor(actor).unwrap().essential;
        initialized += 1;
        if index % 2 == 0 {
            world.start_deferred_kill(actor);
            assert!(!world.kill_actor(actor, Some(player)));
            assert!(!world.actor_ref(actor).unwrap().dead);
            assert_eq!(world.end_deferred_kill(actor), !protected);
        } else {
            assert_eq!(world.kill_actor(actor, Some(player)), !protected);
        }
        let state = world.actor_ref(actor).unwrap();
        assert_eq!(state.dead, !protected);
        if protected {
            essential += 1;
            assert_eq!(state.killer, None);
        } else {
            killed += 1;
            assert_eq!(state.killer, Some(player));
        }
    }
    assert!(initialized > 1000);
    assert!(killed > 0 && essential > 0);
    let hash = world.state_hash();
    let bytes = world.save_state();
    world.load_state(&bytes).unwrap();
    assert_eq!(world.state_hash(), hash);
    assert_eq!(world.save_state(), bytes);
    eprintln!("Death lifecycle: {initialized} actors, {killed} deaths, {essential} essential, {suppressed} suppressed spawns");
}
