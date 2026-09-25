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
use soe_game::quest;
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
const FEED_IDLE: u32 = 0xA12;
const LINK_KEYWORD: u32 = 0xA13;
const BLOOD_SHADER: u32 = 0xA14;
const CHANGE_EFFECT: u32 = 0xA15;
const SOUL_CAIRN_MUSIC: u32 = 0xA16;
const TURN_QUEST: u32 = 0xA17;
const FEED_SCENE: u32 = 0xA18;
const HUNT_PACKAGE: u32 = 0xA19;
const HAND_SPELL: u32 = 0xA1A;

fn le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

/// A minimal `SPIT` block with the spell type at byte 8, matching
/// `crates/game/tests/ui_03_magic_menu.rs`. Type 0 is a hand-castable Spell.
fn spit(spell_type: u32) -> Vec<u8> {
    let mut data = vec![0_u8; 36];
    data[8..12].copy_from_slice(&le(spell_type));
    data
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

/// An `XLKR` pair, the keyword then the reference it links to.
fn xlkr(keyword: u32, reference: u32) -> Vec<u8> {
    let mut d = le(keyword).to_vec();
    d.extend_from_slice(&le(reference));
    d
}

fn actor_ref(id: u32, edid: &str, base: u32) -> Record {
    Record::new(T(b"ACHR"), id)
        .zsub(EDID, edid)
        .sub(T(b"NAME"), le(base))
        .sub(T(b"DATA"), vec![0u8; 24])
}

fn clothing(id: u32, edid: &str) -> Record {
    let mut body = le(1 << 2).to_vec();
    body.extend_from_slice(&le(2));
    Record::new(T(b"ARMO"), id)
        .zsub(EDID, edid)
        .sub(T(b"DNAM"), le(0))
        .sub(T(b"BOD2"), body)
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
    p.push_top(
        Record::new(T(b"SPEL"), HAND_SPELL)
            .zsub(EDID, "TestFlames")
            .sub(T(b"SPIT"), spit(0)),
    );
    p.push_top(Record::new(T(b"MISC"), ASH_PILE).zsub(EDID, "AshPileBase"));
    p.push_top(Record::new(T(b"KYWD"), VAMPIRE_KEYWORD).zsub(EDID, "VampireDress"));
    p.push_top(clothing(VAMPIRE_ARMOR, "VampireRobes").sub(T(b"KWDA"), le(VAMPIRE_KEYWORD)));
    p.push_top(clothing(PLAIN_ARMOR, "FarmClothes"));
    p.push_top(Record::new(T(b"IDLE"), FEED_IDLE).zsub(EDID, "IdleVampireFeed"));
    p.push_top(Record::new(T(b"KYWD"), LINK_KEYWORD).zsub(EDID, "LinkCoffin"));
    p.push_top(Record::new(T(b"EFSH"), BLOOD_SHADER).zsub(EDID, "BloodShader"));
    p.push_top(Record::new(T(b"RFCT"), CHANGE_EFFECT).zsub(EDID, "VampireChangeFX"));
    p.push_top(Record::new(T(b"MUSC"), SOUL_CAIRN_MUSIC).zsub(EDID, "SoulCairnMusic"));
    // A quest with a scene and a package that name it as their owner, the
    // shape every quest fragment script asks GetOwningQuest about.
    p.push_top(
        quest::with_stage(
            quest::with_stage(
                quest::build_record(TURN_QUEST, "VampireTurnQuest", 0),
                10,
                0,
                0,
                "ten",
            ),
            20,
            0,
            0,
            "twenty",
        )
        .sub(T(b"DNAM"), le(0))
        .zsub(T(b"ALID"), ""),
    );
    p.push_top(
        Record::new(T(b"SCEN"), FEED_SCENE)
            .zsub(EDID, "FeedScene")
            .sub(T(b"PNAM"), le(TURN_QUEST)),
    );
    p.push_top(
        Record::new(T(b"PACK"), HUNT_PACKAGE)
            .zsub(EDID, "HuntPackage")
            .sub(T(b"QNAM"), le(TURN_QUEST)),
    );

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
                .sub(T(b"VMAD"), watcher.write(T(b"ACHR")))
                .sub(T(b"XLKR"), xlkr(0, PLAYER))
                .sub(T(b"XLKR"), xlkr(LINK_KEYWORD, HORSE))
                .sub(T(b"XLKR"), xlkr(LINK_KEYWORD, DRAGON)),
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
    let effect = ScriptBuilder::new("DrainEffectScript", "ActiveMagicEffect")
        .function(event_tracing("OnUpdate", &[], "effect update"))
        .bytes();
    vec![watch, effect]
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

/// CK DispelSpell removes active effects. Knowing or equipping a spell
/// does not create an active effect, so dispelling preserves both states.
#[test]
fn dispelling_never_removes_known_or_equipped_spells() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let drain = obj(&mut g, "DrainLife");
    call(&mut g, &vampire, "AddSpell", &[drain.clone()]);
    call(
        &mut g,
        &vampire,
        "EquipSpell",
        &[drain.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &vampire, "DispelSpell", &[drain.clone()]),
        Value::Bool(false)
    );
    for all in [false, true] {
        if all {
            call(&mut g, &vampire, "DispelAllSpells", &[]);
        }
        assert_eq!(
            call(&mut g, &vampire, "HasSpell", &[drain.clone()]),
            Value::Bool(true)
        );
        assert_eq!(
            call(&mut g, &vampire, "IsEquipped", &[drain.clone()]),
            Value::Bool(true)
        );
    }
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
    call(&mut g, &vampire, "RemoveSpell", &[drain.clone()]);
    assert_eq!(
        call(&mut g, &vampire, "HasSpell", &[drain]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// UI-03: EquipSpell with a hand source (0 left, 1 right) and the magic
/// menu's `World::equip_hand_spell` land in the same state. A script that
/// calls EquipSpell sees the change through GetEquippedSpell, and the menu
/// reading `World::hand_spell` after a script equip sees it too, so neither
/// can equip a hand the other does not know about.
#[test]
fn ui_03_equip_spell_native_and_the_magic_menu_share_one_hand_state() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let spell = obj(&mut g, "TestFlames");
    let vampire_id = form(&g, "VampireRef");
    let spell_id = form(&g, "TestFlames");

    call(&mut g, &vampire, "AddSpell", &[spell.clone()]);
    call(
        &mut g,
        &vampire,
        "EquipSpell",
        &[spell.clone(), Value::Int(1)],
    );

    assert_eq!(
        call(&mut g, &vampire, "GetEquippedSpell", &[Value::Int(1)]),
        spell,
        "the native getter must read back what EquipSpell just wrote"
    );
    let via_menu = g
        .world_mut()
        .hand_spell(vampire_id, soe_game::combat::Hand::Right);
    assert_eq!(
        via_menu,
        Some(spell_id),
        "the menu reads the same hand state the native equip just set"
    );
    assert_eq!(
        call(&mut g, &vampire, "GetEquippedSpell", &[Value::Int(0)]),
        Value::None,
        "the left hand was never equipped"
    );

    call(&mut g, &vampire, "UnequipSpell", &[spell, Value::Int(1)]);
    let via_menu_after_unequip = g
        .world_mut()
        .hand_spell(vampire_id, soe_game::combat::Hand::Right);
    assert_eq!(
        via_menu_after_unequip, None,
        "UnequipSpell must clear the same hand state EquipSpell set"
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

// ----- the animation graph on a reference -----

/// SetAnimationVariableFloat, Int and Bool "set the value of a variable on
/// the reference's animation graph", and the Get pair "fetches" it back. A
/// name the graph never saw reads as the quiet value.
#[test]
fn animation_variables_round_trip_per_type() {
    let mut g = game();
    let dragon = obj(&mut g, "DragonRef");
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableFloat",
            &[Value::string("Speed")]
        ),
        Value::Float(0.0)
    );
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableInt",
            &[Value::string("Stage")]
        ),
        Value::Int(0)
    );
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableBool",
            &[Value::string("IsRiding")]
        ),
        Value::Bool(false)
    );

    call(
        &mut g,
        &dragon,
        "SetAnimationVariableFloat",
        &[Value::string("Speed"), Value::Float(2.5)],
    );
    call(
        &mut g,
        &dragon,
        "SetAnimationVariableInt",
        &[Value::string("Stage"), Value::Int(4)],
    );
    call(
        &mut g,
        &dragon,
        "SetAnimationVariableBool",
        &[Value::string("IsRiding"), Value::Bool(true)],
    );
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableFloat",
            &[Value::string("Speed")]
        ),
        Value::Float(2.5)
    );
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableInt",
            &[Value::string("Stage")]
        ),
        Value::Int(4)
    );
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableBool",
            &[Value::string("IsRiding")]
        ),
        Value::Bool(true)
    );
    // The name is not case sensitive, the graph holds one entry per name.
    assert_eq!(
        call(
            &mut g,
            &dragon,
            "GetAnimationVariableFloat",
            &[Value::string("speed")]
        ),
        Value::Float(2.5)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SetAlpha "(Float afTargetAlpha, Bool abFade)". Without frames to fade
/// over, the target is reached at once. PlayIdle sends the idle to the
/// actor's graph, which the engine delivers as an animation event named
/// after the idle.
#[test]
fn alpha_and_idles_reach_the_reference() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let idle = obj(&mut g, "IdleVampireFeed");
    let id = form(&g, "VampireRef");
    assert_eq!(g.world_mut().reference(id).unwrap().alpha, 1.0);
    call(
        &mut g,
        &vampire,
        "SetAlpha",
        &[Value::Float(0.25), Value::Bool(true)],
    );
    assert_eq!(g.world_mut().reference(id).unwrap().alpha, 0.25);

    assert_eq!(
        call(&mut g, &vampire, "PlayIdle", &[idle]),
        Value::Bool(true)
    );
    assert!(
        g.world_mut()
            .animation_events
            .iter()
            .any(|(src, name)| *src == id && name == "IdleVampireFeed"),
        "the idle was not delivered"
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// GetNthLinkedRef walks the XLKR list GetLinkedRef reads the head of, with
/// the same keyword filter. Past the end it is None.
#[test]
fn linked_refs_are_read_by_index_and_keyword() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let horse = obj(&mut g, "HorseRef");
    let dragon = obj(&mut g, "DragonRef");
    let player = obj(&mut g, "PlayerRef");
    let keyword = obj(&mut g, "LinkCoffin");
    assert_eq!(
        call(
            &mut g,
            &vampire,
            "GetNthLinkedRef",
            &[Value::Int(0), Value::None]
        ),
        player
    );
    assert_eq!(
        call(
            &mut g,
            &vampire,
            "GetNthLinkedRef",
            &[Value::Int(1), Value::None]
        ),
        horse
    );
    assert_eq!(
        call(
            &mut g,
            &vampire,
            "GetNthLinkedRef",
            &[Value::Int(0), keyword.clone()]
        ),
        horse
    );
    assert_eq!(
        call(
            &mut g,
            &vampire,
            "GetNthLinkedRef",
            &[Value::Int(1), keyword.clone()]
        ),
        dragon
    );
    assert_eq!(
        call(
            &mut g,
            &vampire,
            "GetNthLinkedRef",
            &[Value::Int(2), keyword]
        ),
        Value::None
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// PlaceActorAtMe places one actor at the reference, PlaceAtMe with the
/// actor the leveled actor lists want.
#[test]
fn place_actor_at_me_makes_one_actor() {
    let mut g = game();
    let vampire = obj(&mut g, "VampireRef");
    let base = obj(&mut g, "DragonBase");
    let placed = call(&mut g, &vampire, "PlaceActorAtMe", &[base, Value::Int(1)]);
    assert!(matches!(placed, Value::Object(_)), "{placed:?}");
    assert_eq!(call(&mut g, &placed, "IsEnabled", &[]), Value::Bool(true));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// The shader, effect and music calls the DLC scripts make most often are
/// registered and quiet, so a vanilla flow runs past them rather than
/// stopping on an unresolved call.
#[test]
fn the_effect_systems_take_their_calls_quietly() {
    let mut g = game();
    let shader = obj(&mut g, "BloodShader");
    let effect = obj(&mut g, "VampireChangeFX");
    let music = obj(&mut g, "SoulCairnMusic");
    let vampire = obj(&mut g, "VampireRef");
    call(
        &mut g,
        &shader,
        "Play",
        &[vampire.clone(), Value::Float(2.0)],
    );
    call(&mut g, &shader, "Stop", &[vampire.clone()]);
    call(
        &mut g,
        &effect,
        "Play",
        &[vampire.clone(), Value::Float(2.0)],
    );
    call(&mut g, &effect, "Stop", &[vampire.clone()]);
    call(&mut g, &music, "Add", &[]);
    call(&mut g, &music, "Remove", &[]);
    call(&mut g, &vampire, "Reset", &[]);
    call(&mut g, &vampire, "StopTranslation", &[]);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- quest-owned fragments and active magic effects -----

/// GetOwningQuest on a quest-owned fragment script. A scene names its parent
/// quest in PNAM and a package its owner quest in QNAM, which is what the
/// 1,815 DLC call sites are asking for. A topic info takes its quest from
/// the DIAL above it and answers None until the dialogue view lands.
#[test]
fn fragment_scripts_find_their_owning_quest() {
    let mut g = game();
    let scene = obj(&mut g, "FeedScene");
    let package = obj(&mut g, "HuntPackage");
    let want = obj(&mut g, "VampireTurnQuest");
    assert_eq!(call(&mut g, &scene, "GetOwningQuest", &[]), want);
    assert_eq!(call(&mut g, &package, "GetOwningQuest", &[]), want);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// An ActiveMagicEffect is not a Form, so the registrations the registry
/// holds on Form are out of its reach. They key off the calling object, so
/// the effect gets its own and OnUpdate arrives on the effect's script.
#[test]
fn an_active_magic_effect_registers_for_its_own_update() {
    let mut g = game();
    g.start();
    let id =
        g.vm.create_object_of("DrainEffectScript", "ActiveMagicEffect");
    g.vm.attach(id, "DrainEffectScript").unwrap();
    let effect = Value::Object(id);
    call(
        &mut g,
        &effect,
        "RegisterForSingleUpdate",
        &[Value::Float(1.0)],
    );
    assert!(
        g.traces().iter().all(|t| t != "effect update"),
        "the update arrived before the clock moved"
    );
    g.tick(2.0);
    g.run_until_idle(64);
    assert!(
        g.traces().iter().any(|t| t == "effect update"),
        "{:?}",
        g.traces()
    );
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

#[test]
fn quest_getters_agree_after_lower_stage_and_reset() {
    let mut g = game();
    let quest = obj(&mut g, "VampireTurnQuest");
    for stage in [20, 10] {
        assert_eq!(
            call(&mut g, &quest, "SetStage", &[Value::Int(stage)]),
            Value::Bool(true)
        );
        for getter in ["GetStage", "GetCurrentStageID"] {
            assert_eq!(call(&mut g, &quest, getter, &[]), Value::Int(20));
        }
    }
    call(&mut g, &quest, "Reset", &[]);
    for getter in ["GetStage", "GetCurrentStageID"] {
        assert_eq!(call(&mut g, &quest, getter, &[]), Value::Int(0));
    }
}

#[test]
fn game_tick_supplies_timescale_and_menu_pause_to_waits() {
    let mut g = game();
    let script = ScriptBuilder::new("GameClockProbe", "")
        .function(
            FunctionBuilder::new("Run", "Int")
                .global()
                .ins(
                    CallStatic,
                    vec![
                        s("Utility"),
                        s("WaitGameTime"),
                        ident("::nonevar"),
                        soe_papyrus::asm::f(1.0),
                    ],
                )
                .ins(Return, vec![i(7)])
                .build(),
        )
        .bytes();
    g.load_script(&script).unwrap();
    let thread = g.vm.call_global("GameClockProbe", "Run", &[]).unwrap();
    g.run_until_idle(64);
    g.tick_menu_mode(1000.0, true);
    assert_eq!(g.world().clock.real_seconds, 0.0);
    assert_eq!(g.vm.thread_result(thread), None);
    g.tick(90.0);
    assert_eq!(g.vm.thread_result(thread), None);
    g.world_mut().clock.timescale = 40.0;
    g.tick(44.0);
    assert_eq!(g.vm.thread_result(thread), None);
    g.tick(1.1);
    assert_eq!(g.vm.thread_result(thread), Some(Value::Int(7)));
}
