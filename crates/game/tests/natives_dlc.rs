// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cloned_ref_to_slice_refs
)]

//! Scenarios for the natives Dawnguard, HearthFires and Dragonborn added to
//! the base classes: the vampirism and lycanthropy senders with the events
//! they deliver, the dragon riding flags, the deferred kill, shouts, the
//! behavior graph variables and the rest of what the DLC scripts call. Every
//! native is driven through a probe script assembled per call, so the path
//! under test is the same `CallMethod` a vanilla script takes. The behaviour
//! each test asserts is the one the Creation Kit class page states, quoted in
//! the test's doc comment. None of these functions has a page of its own in
//! the corpus, so `Actor_Script.wikitext`, `Game_Script.wikitext` and
//! `ObjectReference_Script.wikitext` are the source.

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::harness::{self, Scenario};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{b, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Function;
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const PLAYER: u32 = 0x14;
const PLAYER_BASE: u32 = 0xA00;
const VAMPIRE: u32 = 0xA01;
const VAMPIRE_BASE: u32 = 0xA02;
const DRAGON: u32 = 0xA03;
const DRAGON_BASE: u32 = 0xA04;
const HORSE: u32 = 0xA05;
const HORSE_BASE: u32 = 0xA06;
const SHOUT_A: u32 = 0xA07;
const SHOUT_B: u32 = 0xA08;
const EYES: u32 = 0xA09;
const VAMPIRE_RACE: u32 = 0xA0A;
const DRAIN_SPELL: u32 = 0xA0B;
const ASH_PILE: u32 = 0xA0C;
const VAMPIRE_KEYWORD: u32 = 0xA0D;
const VAMPIRE_ARMOR: u32 = 0xA0E;
const PLAIN_ARMOR: u32 = 0xA0F;
const INSIDE: u32 = 0xA10;
const OUTSIDE: u32 = 0xA11;

fn le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn acbs(actor_flags: u32) -> Vec<u8> {
    let mut d = le(actor_flags).to_vec();
    d.extend_from_slice(&[0; 6]);
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&[0; 12]);
    d
}

/// A cell with its references. `interior` sets the low bit of `DATA`, which
/// is the flag `CanFlyHere` reads through `cell_is_interior`.
fn cell(p: &mut Plugin, id: u32, edid: &str, interior: bool, refs: Vec<Record>) -> Group {
    let data = if interior { [1u8, 0] } else { [0u8, 0] };
    let rec = Record::new(T(b"CELL"), id)
        .zsub(EDID, edid)
        .sub(T(b"DATA"), data);
    let rec = p.push_record(rec);
    let mut temp = Group::new(GroupKind::CellTemporary(id));
    for r in refs {
        let idx = p.push_record(r);
        temp.entries.push(Entry::Record(idx));
    }
    let mut children = Group::new(GroupKind::CellChildren(id));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(rec));
    sub.entries.push(Entry::Group(children));
    sub
}

fn actor_ref(id: u32, edid: &str, base: u32) -> Record {
    Record::new(T(b"ACHR"), id)
        .zsub(EDID, edid)
        .sub(T(b"NAME"), le(base))
        .sub(T(b"DATA"), vec![0u8; 24])
}

fn test_plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    for (id, edid) in [
        (PLAYER_BASE, "PlayerBase"),
        (VAMPIRE_BASE, "VampireBase"),
        (DRAGON_BASE, "DragonBase"),
        (HORSE_BASE, "HorseBase"),
    ] {
        p.push_top(
            Record::new(T(b"NPC_"), id)
                .zsub(EDID, edid)
                .sub(T(b"ACBS"), acbs(0)),
        );
    }
    p.push_top(Record::new(T(b"SHOU"), SHOUT_A).zsub(EDID, "ShoutDrainVitality"));
    p.push_top(Record::new(T(b"SHOU"), SHOUT_B).zsub(EDID, "ShoutSoulTear"));
    p.push_top(Record::new(T(b"TXST"), EYES).zsub(EDID, "VampireEyes"));
    p.push_top(Record::new(T(b"RACE"), VAMPIRE_RACE).zsub(EDID, "VampireLordRace"));
    p.push_top(Record::new(T(b"SPEL"), DRAIN_SPELL).zsub(EDID, "DrainLife"));
    p.push_top(Record::new(T(b"MISC"), ASH_PILE).zsub(EDID, "AshPileBase"));
    p.push_top(Record::new(T(b"KYWD"), VAMPIRE_KEYWORD).zsub(EDID, "VampireDress"));
    p.push_top(
        Record::new(T(b"ARMO"), VAMPIRE_ARMOR)
            .zsub(EDID, "VampireRobes")
            .sub(T(b"KWDA"), le(VAMPIRE_KEYWORD)),
    );
    p.push_top(Record::new(T(b"ARMO"), PLAIN_ARMOR).zsub(EDID, "FarmClothes"));

    let watcher = Vmad {
        scripts: vec![ScriptEntry::new("VampireWatchScript")],
        ..Default::default()
    };
    let inside = cell(
        &mut p,
        INSIDE,
        "VampireLair",
        true,
        vec![
            actor_ref(PLAYER, "PlayerRef", PLAYER_BASE),
            actor_ref(VAMPIRE, "VampireRef", VAMPIRE_BASE)
                .sub(T(b"VMAD"), watcher.write(T(b"ACHR"))),
        ],
    );
    let outside = cell(
        &mut p,
        OUTSIDE,
        "DragonPeak",
        false,
        vec![
            actor_ref(DRAGON, "DragonRef", DRAGON_BASE),
            actor_ref(HORSE, "HorseRef", HORSE_BASE),
        ],
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(inside));
    block.entries.push(Entry::Group(outside));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

/// An event handler that traces a fixed message.
fn event_tracing(name: &str, params: &[(&str, &str)], message: &str) -> Function {
    let mut fun = FunctionBuilder::new(name, "None");
    for (p, t) in params {
        fun = fun.param(p, t);
    }
    fun.ins(
        CallStatic,
        vec![s("Debug"), s("Trace"), ident("::nonevar"), s(message), i(0)],
    )
    .build()
}

/// The script on the vampire: it hears both state changes, and `TurnToVampire`
/// is the part a Dawnguard quest fragment plays when the turn completes.
fn scripts() -> Vec<Vec<u8>> {
    let watch = ScriptBuilder::new("VampireWatchScript", "Actor")
        .function(event_tracing(
            "OnVampirismStateChanged",
            &[("abIsVampire", "Bool")],
            "vampirism changed",
        ))
        .function(event_tracing(
            "OnLycanthropyStateChanged",
            &[("abIsWerewolf", "Bool")],
            "lycanthropy changed",
        ))
        .function(
            FunctionBuilder::new("TurnToVampire", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("SendVampirismStateChanged"),
                        ident("self"),
                        ident("::nonevar"),
                        b(true),
                    ],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("TurnToWerewolf", "None")
                .ins(
                    CallMethod,
                    vec![
                        s("SendLycanthropyStateChanged"),
                        ident("self"),
                        ident("::nonevar"),
                        b(true),
                    ],
                )
                .build(),
        )
        .bytes();
    vec![watch]
}

fn game() -> Game {
    let bytes = test_plugin().to_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut order = LoadOrder::new();
    order.add("Dlc.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for script in scripts() {
        game.load_script(&script).unwrap();
    }
    game.bind();
    game
}

static PROBES: AtomicUsize = AtomicUsize::new(0);

fn type_of(v: &Value) -> &'static str {
    match v {
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Bool(_) => "Bool",
        Value::Str(_) => "String",
        _ => "Form",
    }
}

/// Call `name` on `target` through a one-off script, the way a vanilla
/// script's `CallMethod` would, and hand back the result.
fn call(game: &mut Game, target: &Value, name: &str, args: &[Value]) -> Value {
    let n = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("DlcProbe{n}");
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .param("t", "Form")
        .local("r", "Var");
    let mut operands = vec![s(name), ident("t"), ident("r")];
    for (k, a) in args.iter().enumerate() {
        let pname = format!("a{k}");
        fun = fun.param(&pname, type_of(a));
        operands.push(ident(&pname));
    }
    let fun = fun
        .ins(CallMethod, operands)
        .ins(Return, vec![ident("r")])
        .build();
    let bytes = ScriptBuilder::new(&script, "Form").function(fun).bytes();
    game.load_script(&bytes).unwrap();
    let mut all = vec![target.clone()];
    all.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &all).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn call_global(game: &mut Game, class: &str, name: &str, args: &[Value]) -> Value {
    let n = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("DlcProbe{n}");
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .local("r", "Var");
    let mut operands = vec![s(class), s(name), ident("r")];
    for (k, a) in args.iter().enumerate() {
        let pname = format!("a{k}");
        fun = fun.param(&pname, type_of(a));
        operands.push(ident(&pname));
    }
    let fun = fun
        .ins(CallStatic, operands)
        .ins(Return, vec![ident("r")])
        .build();
    let bytes = ScriptBuilder::new(&script, "Form").function(fun).bytes();
    game.load_script(&bytes).unwrap();
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn obj(game: &mut Game, edid: &str) -> Value {
    let id = game.form(edid).unwrap_or_else(|| panic!("no form {edid}"));
    Value::Object(game.object_of_form(id))
}

fn form(game: &Game, edid: &str) -> soe_esm::GlobalFormId {
    game.form(edid).unwrap_or_else(|| panic!("no form {edid}"))
}

fn errors(game: &Game) -> Vec<String> {
    game.traces()
        .into_iter()
        .filter(|t| t.contains("no function") || t.contains("no global") || t.contains("error"))
        .collect()
}

// ----- vampirism and lycanthropy -----

/// SendVampirismStateChanged "Tells anyone who cares that the vampirism state
/// of this actor has changed", which the engine delivers as
/// OnVampirismStateChanged to the scripts on that actor. SendLycanthropy
/// StateChanged is the same for OnLycanthropyStateChanged.
#[test]
fn the_state_change_senders_reach_the_actors_own_scripts() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    call(
        &mut g,
        &vampire,
        "SendVampirismStateChanged",
        &[Value::Bool(true)],
    );
    assert!(
        g.traces().iter().any(|t| t == "vampirism changed"),
        "{:?}",
        g.traces()
    );
    let id = form(&g, "VampireRef");
    assert!(g.world_mut().actor(id).unwrap().vampire);

    call(
        &mut g,
        &vampire,
        "SendLycanthropyStateChanged",
        &[Value::Bool(true)],
    );
    assert!(g.traces().iter().any(|t| t == "lycanthropy changed"));
    assert!(g.world_mut().actor(id).unwrap().werewolf);

    // Cured again, and the event says so a second time.
    call(
        &mut g,
        &vampire,
        "SendVampirismStateChanged",
        &[Value::Bool(false)],
    );
    assert!(!g.world_mut().actor(id).unwrap().vampire);
    assert_eq!(
        g.traces()
            .iter()
            .filter(|t| *t == "vampirism changed")
            .count(),
        2
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// An actor with no script of its own still takes the call. The event has
/// nowhere to land, which is what "anyone who cares" allows for.
#[test]
fn a_state_change_on_a_scriptless_actor_is_quiet() {
    let mut g = game();
    let horse = obj(&mut g, "HorseRef");
    call(
        &mut g,
        &horse,
        "SendVampirismStateChanged",
        &[Value::Bool(true)],
    );
    let id = form(&g, "HorseRef");
    assert!(g.world_mut().actor(id).unwrap().vampire);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// The Dawnguard turn as the harness drives it: a quest fragment calls
/// SendVampirismStateChanged on the actor, the actor's script hears the
/// event. The scenario is a preserve case, vanilla logs the same trace.
#[test]
fn the_vampire_turn_runs_as_a_harness_scenario() {
    let mut g = game();
    g.start();
    let scenario = Scenario::parse(
        "# preserve: Dawnguard's vampire turn tells the actor's scripts the state changed\n\
         event VampireRef TurnToVampire\n\
         expect trace \"vampirism changed\"\n\
         event VampireRef TurnToWerewolf\n\
         expect trace \"lycanthropy changed\"\n",
    )
    .unwrap();
    let report = harness::run(&mut g, &scenario);
    assert_eq!(report.checks, 2);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let id = form(&g, "VampireRef");
    assert!(g.world_mut().actor(id).unwrap().vampire);
    assert!(g.world_mut().actor(id).unwrap().werewolf);
}

// ----- dragon riding -----

/// SetAllowFlying "Flags this actor as whether they are allowed to fly or
/// not", IsAllowedToFly reads it back, and CanFlyHere ("Can this actor fly
/// here?") wants the sky as well as the permission. SetAllowFlyingEx takes
/// the same first argument plus the crash and search permissions.
#[test]
fn flying_needs_the_permission_and_the_sky() {
    let mut g = game();
    let dragon = obj(&mut g, "DragonRef");
    let vampire = obj(&mut g, "VampireRef");
    assert_eq!(
        call(&mut g, &dragon, "IsAllowedToFly", &[]),
        Value::Bool(false)
    );
    assert_eq!(call(&mut g, &dragon, "CanFlyHere", &[]), Value::Bool(false));

    call(&mut g, &dragon, "SetAllowFlying", &[Value::Bool(true)]);
    assert_eq!(
        call(&mut g, &dragon, "IsAllowedToFly", &[]),
        Value::Bool(true)
    );
    assert_eq!(call(&mut g, &dragon, "CanFlyHere", &[]), Value::Bool(true));

    // The same permission indoors is not a place to fly.
    call(
        &mut g,
        &vampire,
        "SetAllowFlyingEx",
        &[Value::Bool(true), Value::Bool(false), Value::Bool(false)],
    );
    assert_eq!(
        call(&mut g, &vampire, "IsAllowedToFly", &[]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &vampire, "CanFlyHere", &[]),
        Value::Bool(false)
    );

    call(&mut g, &dragon, "SetAllowFlying", &[Value::Bool(false)]);
    assert_eq!(call(&mut g, &dragon, "CanFlyHere", &[]), Value::Bool(false));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// IsFlying "Is this actor currently flying?" is false while the engine has
/// no flight to report.
#[test]
fn is_flying_is_false_without_a_flight_model() {
    let mut g = game();
    let dragon = obj(&mut g, "DragonRef");
    call(&mut g, &dragon, "SetAllowFlying", &[Value::Bool(true)]);
    assert_eq!(call(&mut g, &dragon, "IsFlying", &[]), Value::Bool(false));
}

// ----- the deferred kill -----

/// EndDeferredKill "Removes the actor from a Deferred Kill state". While the
/// state is on, the kill the DLC scripts ask for waits, which is the point of
/// the pair. KillSilent "Kills this actor without a kill event with the
/// killer being the guilty party".
#[test]
fn a_deferred_kill_holds_the_death_until_it_ends() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let player = obj(&mut g, "PlayerRef");
    let id = form(&g, "VampireRef");

    call(&mut g, &vampire, "StartDeferredKill", &[]);
    call(&mut g, &vampire, "Kill", &[player.clone()]);
    assert_eq!(call(&mut g, &vampire, "IsDead", &[]), Value::Bool(false));
    call(&mut g, &vampire, "KillSilent", &[player.clone()]);
    assert_eq!(call(&mut g, &vampire, "IsDead", &[]), Value::Bool(false));

    call(&mut g, &vampire, "EndDeferredKill", &[]);
    call(&mut g, &vampire, "KillSilent", &[player.clone()]);
    assert_eq!(call(&mut g, &vampire, "IsDead", &[]), Value::Bool(true));
    let player_id = form(&g, "PlayerRef");
    assert_eq!(g.world_mut().actor(id).unwrap().killer, Some(player_id));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- shouts -----

/// AddShout "Adds the specified shout to this actor" and returns whether it
/// was new, RemoveShout "Removes the specified shout from this actor",
/// EquipShout "Forces this actor to equip this shout" and GetEquippedShout
/// "Obtains the currently equipped shout on this actor".
#[test]
fn shouts_are_added_equipped_and_removed() {
    let mut g = game();
    let dragon = obj(&mut g, "DragonRef");
    let drain = obj(&mut g, "ShoutDrainVitality");
    let tear = obj(&mut g, "ShoutSoulTear");
    assert_eq!(
        call(&mut g, &dragon, "AddShout", &[drain.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &dragon, "AddShout", &[drain.clone()]),
        Value::Bool(false)
    );
    assert_eq!(call(&mut g, &dragon, "GetEquippedShout", &[]), Value::None);

    call(&mut g, &dragon, "EquipShout", &[tear.clone()]);
    assert_eq!(call(&mut g, &dragon, "GetEquippedShout", &[]), tear);

    // Removing the equipped shout takes it out of the hand too.
    assert_eq!(
        call(&mut g, &dragon, "RemoveShout", &[tear.clone()]),
        Value::Bool(true)
    );
    assert_eq!(call(&mut g, &dragon, "GetEquippedShout", &[]), Value::None);
    assert_eq!(
        call(&mut g, &dragon, "RemoveShout", &[tear]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &dragon, "RemoveShout", &[drain]),
        Value::Bool(true)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- spells, race, eyes, ash -----

/// DispelSpell "Dispel a spell from this actor", DispelAllSpells "Dispel all
/// spell from this actor", EquipSpell "Forces this actor to equip thes spell,
/// in the specified source" and UnequipSpell take it back out.
#[test]
fn spells_are_dispelled_and_equipped() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let drain = obj(&mut g, "DrainLife");
    call(&mut g, &vampire, "AddSpell", &[drain.clone()]);
    assert_eq!(
        call(&mut g, &vampire, "HasSpell", &[drain.clone()]),
        Value::Bool(true)
    );

    call(
        &mut g,
        &vampire,
        "EquipSpell",
        &[drain.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &vampire, "IsEquipped", &[drain.clone()]),
        Value::Bool(true)
    );
    call(
        &mut g,
        &vampire,
        "UnequipSpell",
        &[drain.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &vampire, "IsEquipped", &[drain.clone()]),
        Value::Bool(false)
    );

    assert_eq!(
        call(&mut g, &vampire, "DispelSpell", &[drain.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &vampire, "DispelSpell", &[drain.clone()]),
        Value::Bool(false)
    );
    call(&mut g, &vampire, "AddSpell", &[drain.clone()]);
    call(&mut g, &vampire, "DispelAllSpells", &[]);
    assert_eq!(
        call(&mut g, &vampire, "HasSpell", &[drain]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SetRace "Sets the actor's race", which GetRace reads back. SetEyeTexture
/// "Forces a new eye texture on the actor" and AttachAshPile "Attaches an ash
/// pile to this actor using the passed-in form as a base".
#[test]
fn the_vampire_lord_takes_a_race_eyes_and_an_ash_pile() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let race = obj(&mut g, "VampireLordRace");
    let eyes = obj(&mut g, "VampireEyes");
    let ash = obj(&mut g, "AshPileBase");
    let id = form(&g, "VampireRef");

    call(&mut g, &vampire, "SetRace", &[race.clone()]);
    assert_eq!(call(&mut g, &vampire, "GetRace", &[]), race);
    call(&mut g, &vampire, "SetEyeTexture", &[eyes]);
    let eyes_id = form(&g, "VampireEyes");
    assert_eq!(g.world_mut().actor(id).unwrap().eye_texture, Some(eyes_id));
    call(&mut g, &vampire, "AttachAshPile", &[ash]);
    let ash_id = form(&g, "AshPileBase");
    assert_eq!(g.world_mut().actor(id).unwrap().ash_pile, Some(ash_id));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// WornHasKeyword asks the keywords of what the actor has equipped, not of
/// the actor. The Dawnguard scripts use it to tell a dressed vampire apart.
#[test]
fn worn_keywords_come_from_the_equipped_items() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let keyword = obj(&mut g, "VampireDress");
    let robes = obj(&mut g, "VampireRobes");
    let clothes = obj(&mut g, "FarmClothes");
    assert_eq!(
        call(&mut g, &vampire, "WornHasKeyword", &[keyword.clone()]),
        Value::Bool(false)
    );

    call(
        &mut g,
        &vampire,
        "AddItem",
        &[clothes.clone(), Value::Int(1), Value::Bool(true)],
    );
    call(
        &mut g,
        &vampire,
        "EquipItem",
        &[clothes, Value::Bool(false), Value::Bool(true)],
    );
    assert_eq!(
        call(&mut g, &vampire, "WornHasKeyword", &[keyword.clone()]),
        Value::Bool(false)
    );

    call(
        &mut g,
        &vampire,
        "AddItem",
        &[robes.clone(), Value::Int(1), Value::Bool(true)],
    );
    call(
        &mut g,
        &vampire,
        "EquipItem",
        &[robes, Value::Bool(false), Value::Bool(true)],
    );
    assert_eq!(
        call(&mut g, &vampire, "WornHasKeyword", &[keyword]),
        Value::Bool(true)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- the rest of the flags -----

/// EnableAI "Enables or disables this actor's AI", SetNoBleedoutRecovery
/// "Sets or clears the no bleedout recovery flag on this actor" with
/// GetNoBleedoutRecovery reading it, and SetCriticalStage "Sets this actor's
/// current critical stage".
#[test]
fn the_actor_flags_the_dlc_scripts_set_read_back() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let id = form(&g, "VampireRef");
    assert!(!g.world_mut().actor(id).unwrap().ai_disabled);

    call(&mut g, &vampire, "EnableAI", &[Value::Bool(false)]);
    assert!(g.world_mut().actor(id).unwrap().ai_disabled);
    call(&mut g, &vampire, "EnableAI", &[Value::Bool(true)]);
    assert!(!g.world_mut().actor(id).unwrap().ai_disabled);

    assert_eq!(
        call(&mut g, &vampire, "GetNoBleedoutRecovery", &[]),
        Value::Bool(false)
    );
    call(
        &mut g,
        &vampire,
        "SetNoBleedoutRecovery",
        &[Value::Bool(true)],
    );
    assert_eq!(
        call(&mut g, &vampire, "GetNoBleedoutRecovery", &[]),
        Value::Bool(true)
    );

    call(&mut g, &vampire, "SetCriticalStage", &[Value::Int(3)]);
    assert_eq!(g.world_mut().actor(id).unwrap().critical_stage, 3);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SetSubGraphFloatVariable "Set a variable on all of an actor's sub graphs",
/// kept per actor under a lowercase name. PlaySubGraphAnimation "Sends the
/// event passed to the actor's sub graphs", which is the delivery an
/// animation event takes, so a registered listener hears it.
#[test]
fn the_behavior_graph_takes_variables_and_events() {
    let mut g = game();
    let dragon = obj(&mut g, "DragonRef");
    let id = form(&g, "DragonRef");
    call(
        &mut g,
        &dragon,
        "SetSubGraphFloatVariable",
        &[Value::string("StaffSpeed"), Value::Float(1.5)],
    );
    assert_eq!(
        g.world_mut()
            .actor(id)
            .unwrap()
            .graph_floats
            .get("staffspeed"),
        Some(&1.5)
    );
    call(
        &mut g,
        &dragon,
        "PlaySubGraphAnimation",
        &[Value::string("RideStart")],
    );
    assert!(
        g.world_mut()
            .animation_events
            .iter()
            .any(|(src, name)| *src == id && name == "RideStart"),
        "the sub graph event was not recorded"
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// IsPlayersLastRiddenHorse pairs with Game.GetPlayersLastRiddenHorse, the
/// HearthFires pair that finds the horse the player left somewhere.
#[test]
fn the_last_ridden_horse_knows_itself() {
    let mut g = game();
    let horse = obj(&mut g, "HorseRef");
    let dragon = obj(&mut g, "DragonRef");
    assert_eq!(
        call(&mut g, &horse, "IsPlayersLastRiddenHorse", &[]),
        Value::Bool(false)
    );
    let id = form(&g, "HorseRef");
    g.world_mut().last_ridden_horse = Some(id);
    assert_eq!(
        call(&mut g, &horse, "IsPlayersLastRiddenHorse", &[]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &dragon, "IsPlayersLastRiddenHorse", &[]),
        Value::Bool(false)
    );
    assert_eq!(
        call_global(&mut g, "Game", "GetPlayersLastRiddenHorse", &[]),
        horse
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// AddPerkPoints "Adds the given number of perk points to the player. Clamped
/// at 255", the Dragonborn black book reward.
#[test]
fn perk_points_add_and_clamp_at_255() {
    let mut g = game();
    call_global(&mut g, "Game", "AddPerkPoints", &[Value::Int(2)]);
    assert_eq!(g.world().perk_points, 2);
    call_global(&mut g, "Game", "AddPerkPoints", &[Value::Int(300)]);
    assert_eq!(g.world().perk_points, 255);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// GetWarmthRating "Obtains the total warmth rating for the actor" is zero
/// without the survival plugin, and the sun positions are zero without a sky
/// model. They are registered so the DLC scripts calling them run on.
#[test]
fn the_systems_without_an_engine_answer_quietly() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    assert_eq!(
        call(&mut g, &vampire, "GetWarmthRating", &[]),
        Value::Float(0.0)
    );
    for name in ["GetSunPositionX", "GetSunPositionY", "GetSunPositionZ"] {
        assert_eq!(call_global(&mut g, "Game", name, &[]), Value::Float(0.0));
    }
    call_global(&mut g, "Game", "SendWereWolfTransformation", &[]);
    call(&mut g, &vampire, "ClearExtraArrows", &[]);
    let player = obj(&mut g, "PlayerRef");
    call(&mut g, &player, "ForceAddRagdollToWorld", &[]);
    call(&mut g, &player, "ForceRemoveRagdollFromWorld", &[]);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// The save carries the DLC state: a world written after the turn reads back
/// with the same flags, and the state hash moves with them.
#[test]
fn the_dlc_state_survives_a_save_and_load() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let id = form(&g, "VampireRef");
    call(
        &mut g,
        &vampire,
        "SendVampirismStateChanged",
        &[Value::Bool(true)],
    );
    call(&mut g, &vampire, "SetCriticalStage", &[Value::Int(2)]);
    call(&mut g, &vampire, "SetAllowFlying", &[Value::Bool(true)]);
    let tear = obj(&mut g, "ShoutSoulTear");
    call(&mut g, &vampire, "AddShout", &[tear]);
    call_global(&mut g, "Game", "AddPerkPoints", &[Value::Int(4)]);
    let before = g.world().state_hash();
    let blob = g.world().save_state();

    let fresh = game();
    fresh.world_mut().load_state(&blob).unwrap();
    let after = fresh.world().state_hash();
    assert_eq!(before.parts(), after.parts());
    let a = fresh.world_mut().actor(id).unwrap().clone();
    assert!(a.vampire);
    assert!(a.allow_flying);
    assert_eq!(a.critical_stage, 2);
    assert_eq!(a.shouts.len(), 1);
    assert_eq!(fresh.world().perk_points, 4);
}
