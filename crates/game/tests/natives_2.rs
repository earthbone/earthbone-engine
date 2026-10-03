// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cloned_ref_to_slice_refs
)]

//! Scenarios for the natives round: leveled lists, locations, factions,
//! actor bases, perks and the actor flags, cells, messages, scenes,
//! sounds, spells, the listener registrations on `Form`, the `Game` state
//! natives, and the dispatch walk that lets `Alias.pex` redeclare a
//! `Form` native. Every native is driven through a probe script assembled
//! per call, so the path under test is the same `CallMethod` a vanilla
//! script takes. The behaviour each test asserts is the one the Creation
//! Kit wiki page states, quoted in the test's doc comment.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use soe_esm::{
    plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::inventory::{Extra, Field, Stack};
use soe_game::quest::{self, stage_flags};
use soe_game::vmad::{AliasScripts, ObjectRef, ScriptEntry, Vmad};
use soe_game::{
    world::{RngStream, SpellArtAction, SpellArtRequest},
    HostRequest,
};
use soe_game::{Game, Handle, Pending};
use soe_papyrus::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::pex::Function;
use soe_papyrus::Opcode::*;
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const HOLD_KEYWORD: u32 = 0x900;
const HOLD_LOC: u32 = 0x901;
const TOWN_LOC: u32 = 0x902;
const INN_LOC: u32 = 0x903;
const OTHER_TOWN_LOC: u32 = 0x904;
const INN_CELL: u32 = 0x905;
const PLAYER_BASE: u32 = 0x906;
const PERK: u32 = 0x907;
const GUARDS: u32 = 0x908;
const MARKER: u32 = 0x909;
const MARKER_BASE: u32 = 0x90A;
const GUARD: u32 = 0x90B;
const GUARD_BASE: u32 = 0x90C;
const GUARD_REF_TYPE: u32 = 0xA31;
const OUTFIT: u32 = 0x90D;
const THIEVES: u32 = 0x90E;
const SHARED_CRIME_GROUP: u32 = 0x9A0;
const RAGS: u32 = 0x90F;
const LOOT_LIST: u32 = 0x910;
const GOLD: u32 = 0x911;
const SWORD: u32 = 0x912;
const SHIELD: u32 = 0x987;
const DAGGER: u32 = 0x988;
const BROKEN_ARMOR: u32 = 0x989;
const WEAPON_KEYWORD: u32 = 0x980;
const CARRIED_ITEMS: u32 = 0x981;
const GOLD_REF: u32 = 0x982;
const OTHER_GOLD_REF: u32 = 0x983;
const CHEST_BASE: u32 = 0x984;
const CHEST: u32 = 0x985;
const CYCLIC_LIST: u32 = 0x986;
const ENEMY_LIST: u32 = 0x913;
const SPELL_LIST: u32 = 0x914;
const FLAMES: u32 = 0x915;
const BOX_MESSAGE: u32 = 0x916;
const NOTE_MESSAGE: u32 = 0x917;
const SCENE: u32 = 0x918;
const SOUND: u32 = 0x919;
const COUNTER: u32 = 0x91A;
const QUEST: u32 = 0x91B;
const WORD: u32 = 0x91C;
const OTHER_CELL: u32 = 0x91D;
const ADD_LIST: u32 = 0x91E;
const MIXED_ADD_LIST: u32 = 0x91F;
const REFERENCE_ADD_LIST: u32 = 0x920;
const DUPLICATE_ADD_LIST: u32 = 0x921;
const EMPTY_ADD_LIST: u32 = 0x922;
const NESTED_ADD_LIST: u32 = 0x923;
const SHARED_ADD_LIST: u32 = 0x924;
const CYCLE_ADD_LIST_A: u32 = 0x925;
const CYCLE_ADD_LIST_B: u32 = 0x926;
const LOOSE_SWORD: u32 = 0x927;
const REPEATED_REFERENCE_ADD_LIST: u32 = 0x928;
const MIXED_REFERENCE_ADD_LIST: u32 = 0x929;
const NESTED_LEVELED_ADD_LIST: u32 = 0x92A;
const DEEP_ADD_LIST: u32 = 0x930;
const DEBUG_QUEST: u32 = 0xA01;
const PLAYER: u32 = 0x14;

fn le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn lvlo(level: u16, form: u32, count: u16) -> Vec<u8> {
    let mut d = level.to_le_bytes().to_vec();
    d.extend_from_slice(&[0, 0]);
    d.extend_from_slice(&le(form));
    d.extend_from_slice(&count.to_le_bytes());
    d.extend_from_slice(&[0, 0]);
    d
}

fn xnam(faction: u32, combat: i32) -> Vec<u8> {
    let mut d = le(faction).to_vec();
    d.extend_from_slice(&0i32.to_le_bytes());
    d.extend_from_slice(&combat.to_le_bytes());
    d
}

fn acbs(flags: u32) -> Vec<u8> {
    let mut d = le(flags).to_vec();
    d.extend_from_slice(&[0; 6]);
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&[0; 12]);
    d
}

fn prkr(perk: u32) -> Vec<u8> {
    let mut d = le(perk).to_vec();
    d.extend_from_slice(&[1, 0, 0, 0]);
    d
}

fn location(id: u32, edid: &str, parent: Option<u32>) -> Record {
    let r = Record::new(T(b"LCTN"), id).zsub(EDID, edid);
    match parent {
        Some(p) => r.sub(T(b"PNAM"), le(p)),
        None => r,
    }
}

fn location_ref_type_entry(ref_type: u32, reference: u32, cell: u32) -> Vec<u8> {
    [le(ref_type), le(reference), le(cell), [0; 4]].concat()
}

fn interior(p: &mut Plugin, cell: Record, refs: Vec<Record>) -> Group {
    let id = cell.form_id;
    let cell = p.push_record(cell);
    let mut temp = Group::new(GroupKind::CellTemporary(id));
    for r in refs {
        let idx = p.push_record(r);
        temp.entries.push(Entry::Record(idx));
    }
    let mut children = Group::new(GroupKind::CellChildren(id));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    sub
}

fn test_plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    p.push_top(Record::new(T(b"KYWD"), HOLD_KEYWORD).zsub(EDID, "LocTypeHold"));
    p.push_top(Record::new(T(b"LCRT"), GUARD_REF_TYPE).zsub(EDID, "GuardRefType"));
    p.push_top(location(HOLD_LOC, "HoldLoc", None).sub(T(b"KWDA"), le(HOLD_KEYWORD)));
    p.push_top(location(TOWN_LOC, "TownLoc", Some(HOLD_LOC)));
    p.push_top(location(INN_LOC, "InnLoc", Some(TOWN_LOC)).sub(
        T(b"ACSR"),
        location_ref_type_entry(GUARD_REF_TYPE, GUARD, INN_CELL),
    ));
    p.push_top(location(OTHER_TOWN_LOC, "OtherTownLoc", Some(HOLD_LOC)));
    p.push_top(Record::new(T(b"PERK"), PERK).zsub(EDID, "TestPerk"));
    p.push_top(
        Record::new(T(b"FACT"), GUARDS)
            .zsub(EDID, "GuardsFaction")
            .sub(T(b"XNAM"), xnam(THIEVES, 2))
            .sub(T(b"DATA"), le(0x40))
            .sub(T(b"CRGR"), le(SHARED_CRIME_GROUP)),
    );
    p.push_top(
        Record::new(T(b"FACT"), THIEVES)
            .zsub(EDID, "ThievesFaction")
            .sub(T(b"DATA"), le(0)),
    );
    p.push_top(
        Record::new(T(b"FLST"), SHARED_CRIME_GROUP)
            .zsub(EDID, "SharedCrimeGroup")
            .sub(T(b"LNAM"), le(THIEVES)),
    );
    p.push_top(Record::new(T(b"OTFT"), OUTFIT).zsub(EDID, "GuardOutfit"));
    p.push_top(Record::new(T(b"OTFT"), RAGS).zsub(EDID, "Rags"));
    p.push_top(
        Record::new(T(b"NPC_"), PLAYER_BASE)
            .zsub(EDID, "PlayerBase")
            .sub(T(b"ACBS"), acbs(0)),
    );
    p.push_top(
        Record::new(T(b"QUST"), DEBUG_QUEST)
            .zsub(EDID, "DebugQuest")
            .sub(T(b"DNAM"), [0; 12])
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0; 4])
            .sub(T(b"ALST"), 7_u32.to_le_bytes()),
    );
    p.push_top(
        Record::new(T(b"NPC_"), GUARD_BASE)
            .zsub(EDID, "GuardBase")
            .zsub(T(b"FULL"), "Guard")
            .sub(
                T(b"ACBS"),
                acbs(
                    soe_game::npc::flags::ESSENTIAL
                        | soe_game::npc::flags::FEMALE
                        | soe_game::npc::flags::UNIQUE,
                ),
            )
            .sub(T(b"PRKR"), prkr(PERK))
            .sub(T(b"CRIF"), le(GUARDS))
            .sub(T(b"DOFT"), le(OUTFIT)),
    );
    p.push_top(Record::new(T(b"STAT"), MARKER_BASE).zsub(EDID, "MapMarker"));
    p.push_top(
        Record::new(T(b"MISC"), GOLD)
            .zsub(EDID, "Gold001")
            .zsub(T(b"FULL"), "Gold"),
    );
    p.push_top(Record::new(T(b"KYWD"), WEAPON_KEYWORD).zsub(EDID, "WeaponTypeTest"));
    p.push_top(
        Record::new(T(b"WEAP"), SWORD)
            .zsub(EDID, "IronSword")
            .sub(T(b"KWDA"), le(WEAPON_KEYWORD)),
    );
    p.push_top(
        Record::new(T(b"WEAP"), DAGGER)
            .zsub(EDID, "IronDagger")
            .sub(T(b"KWDA"), le(WEAPON_KEYWORD)),
    );
    p.push_top(
        Record::new(T(b"ARMO"), SHIELD)
            .zsub(EDID, "IronShield")
            .sub(T(b"DNAM"), le(0))
            .sub(
                T(b"BOD2"),
                [1u32 << 9, 2]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    p.push_top(Record::new(T(b"ARMO"), BROKEN_ARMOR).zsub(EDID, "BrokenArmor"));
    p.push_top(
        Record::new(T(b"FLST"), CARRIED_ITEMS)
            .zsub(EDID, "CarriedItems")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(SWORD)),
    );
    p.push_top(
        Record::new(T(b"FLST"), CYCLIC_LIST)
            .zsub(EDID, "CyclicItems")
            .sub(T(b"LNAM"), le(CYCLIC_LIST)),
    );
    p.push_top(Record::new(T(b"CONT"), CHEST_BASE).zsub(EDID, "QueryChest"));
    p.push_top(
        Record::new(T(b"SPEL"), FLAMES)
            .zsub(EDID, "Flames")
            .sub(T(b"SPIT"), vec![0; 36]),
    );
    p.push_top(
        Record::new(T(b"LVLI"), LOOT_LIST)
            .zsub(EDID, "LootList")
            .sub(T(b"LVLD"), [25u8])
            .sub(T(b"LVLF"), [4u8])
            .sub(T(b"LLCT"), [2u8])
            .sub(T(b"LVLO"), lvlo(1, GOLD, 5))
            .sub(T(b"LVLO"), lvlo(10, SWORD, 1))
            .sub(
                T(b"COED"),
                [
                    0_u32.to_le_bytes(),
                    0_u32.to_le_bytes(),
                    1.25_f32.to_le_bytes(),
                ]
                .concat(),
            ),
    );
    p.push_top(
        Record::new(T(b"LVLN"), ENEMY_LIST)
            .zsub(EDID, "EnemyList")
            .sub(T(b"LVLD"), [0u8])
            .sub(T(b"LVLO"), lvlo(5, GUARD_BASE, 1)),
    );
    p.push_top(
        Record::new(T(b"LVSP"), SPELL_LIST)
            .zsub(EDID, "SpellList")
            .sub(T(b"LVLD"), [0u8])
            .sub(T(b"LVLO"), lvlo(1, FLAMES, 1)),
    );
    p.push_top(
        Record::new(T(b"FLST"), ADD_LIST)
            .zsub(EDID, "AddList")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(SWORD)),
    );
    p.push_top(
        Record::new(T(b"FLST"), MIXED_ADD_LIST)
            .zsub(EDID, "MixedAddList")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(LOOT_LIST)),
    );
    p.push_top(
        Record::new(T(b"FLST"), REFERENCE_ADD_LIST)
            .zsub(EDID, "ReferenceAddList")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(LOOSE_SWORD)),
    );
    p.push_top(
        Record::new(T(b"FLST"), MIXED_REFERENCE_ADD_LIST)
            .zsub(EDID, "MixedReferenceAddList")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(LOOT_LIST))
            .sub(T(b"LNAM"), le(LOOSE_SWORD)),
    );
    p.push_top(
        Record::new(T(b"FLST"), NESTED_LEVELED_ADD_LIST)
            .zsub(EDID, "NestedLeveledAddList")
            .sub(T(b"LNAM"), le(MIXED_ADD_LIST)),
    );
    p.push_top(
        Record::new(T(b"FLST"), REPEATED_REFERENCE_ADD_LIST)
            .zsub(EDID, "RepeatedReferenceAddList")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(LOOSE_SWORD))
            .sub(T(b"LNAM"), le(LOOSE_SWORD)),
    );
    p.push_top(
        Record::new(T(b"FLST"), DUPLICATE_ADD_LIST)
            .zsub(EDID, "DuplicateAddList")
            .sub(T(b"LNAM"), le(GOLD))
            .sub(T(b"LNAM"), le(GOLD)),
    );
    p.push_top(Record::new(T(b"FLST"), EMPTY_ADD_LIST).zsub(EDID, "EmptyAddList"));
    p.push_top(
        Record::new(T(b"FLST"), NESTED_ADD_LIST)
            .zsub(EDID, "NestedAddList")
            .sub(T(b"LNAM"), le(ADD_LIST)),
    );
    p.push_top(
        Record::new(T(b"FLST"), SHARED_ADD_LIST)
            .zsub(EDID, "SharedAddList")
            .sub(T(b"LNAM"), le(ADD_LIST))
            .sub(T(b"LNAM"), le(ADD_LIST)),
    );
    p.push_top(
        Record::new(T(b"FLST"), CYCLE_ADD_LIST_A)
            .zsub(EDID, "CycleAddListA")
            .sub(T(b"LNAM"), le(CYCLE_ADD_LIST_B)),
    );
    p.push_top(
        Record::new(T(b"FLST"), CYCLE_ADD_LIST_B)
            .zsub(EDID, "CycleAddListB")
            .sub(T(b"LNAM"), le(CYCLE_ADD_LIST_A)),
    );
    p.push_top(
        Record::new(T(b"REFR"), LOOSE_SWORD)
            .zsub(EDID, "LooseSwordRef")
            .sub(T(b"NAME"), le(SWORD))
            .sub(T(b"XCNT"), le(2))
            .sub(T(b"XCHG"), le(0x3fc0_0000)),
    );
    for offset in 0..65 {
        let next = if offset == 64 {
            GOLD
        } else {
            DEEP_ADD_LIST + offset + 1
        };
        p.push_top(
            Record::new(T(b"FLST"), DEEP_ADD_LIST + offset)
                .zsub(EDID, &format!("DeepAddList{offset}"))
                .sub(T(b"LNAM"), le(next)),
        );
    }
    p.push_top(
        Record::new(T(b"MESG"), BOX_MESSAGE)
            .zsub(EDID, "BoxMessage")
            .zsub(T(b"DESC"), "Hello box")
            .sub(T(b"DNAM"), le(1)),
    );
    p.push_top(
        Record::new(T(b"MESG"), NOTE_MESSAGE)
            .zsub(EDID, "NoteMessage")
            .zsub(T(b"DESC"), "Hello note")
            .sub(T(b"DNAM"), le(0)),
    );
    p.push_top(
        Record::new(T(b"SCEN"), SCENE)
            .zsub(EDID, "TestScene")
            .sub(T(b"HNAM"), [])
            .zsub(T(b"NAM0"), "First")
            .sub(T(b"NEXT"), [])
            .sub(T(b"HNAM"), [])
            .sub(T(b"ANAM"), 0u16.to_le_bytes())
            .zsub(T(b"NAM0"), "Action")
            .sub(T(b"ALID"), 1i32.to_le_bytes())
            .sub(T(b"INAM"), 0u32.to_le_bytes())
            .sub(T(b"SNAM"), 0u32.to_le_bytes())
            .sub(T(b"ENAM"), 0u32.to_le_bytes())
            .sub(T(b"DATA"), 0x900u32.to_le_bytes())
            .sub(T(b"ANAM"), [])
            .sub(
                T(b"VMAD"),
                Vmad {
                    scripts: vec![ScriptEntry::new("SceneScript")],
                    ..Default::default()
                }
                .write(T(b"SCEN")),
            ),
    );
    p.push_top(Record::new(T(b"SNDR"), SOUND).zsub(EDID, "TestSound"));
    p.push_top(Record::new(T(b"SCRL"), 0xA00).zsub(EDID, "TestScroll"));
    p.push_top(Record::new(T(b"WOOP"), WORD).zsub(EDID, "WordFus"));
    p.push_top(
        Record::new(T(b"GLOB"), COUNTER)
            .zsub(EDID, "Counter")
            .sub(T(b"FNAM"), *b"f")
            .sub(T(b"FLTV"), 0f32.to_le_bytes()),
    );
    let quest_vmad = Vmad {
        aliases: vec![AliasScripts {
            object: ObjectRef {
                form_id: QUEST,
                alias: 1,
            },
            scripts: vec![ScriptEntry::new("WatcherScript")],
        }],
        ..Default::default()
    };
    let q = quest::build_record(QUEST, "LocQuest", quest::flags::START_GAME_ENABLED)
        .sub(T(b"VMAD"), quest_vmad.write(T(b"QUST")));
    let q = quest::with_stage(q, 0, stage_flags::START_UP, 0, "");
    let q = quest::with_objective(q, 10, "Count to three");
    // A location alias starts with ALLS where a reference alias has ALST.
    let q = q
        .sub(T(b"ALLS"), le(0))
        .zsub(T(b"ALID"), "Place")
        .sub(T(b"FNAM"), le(quest::alias_flags::OPTIONAL))
        .sub(T(b"ALED"), Vec::new());
    let q = quest::with_alias(q, 1, "Watcher", Some(GUARD));
    p.push_top(q);

    let zero = vec![0u8; 24];
    let inn = interior(
        &mut p,
        Record::new(T(b"CELL"), INN_CELL)
            .zsub(EDID, "InnCell")
            .sub(T(b"DATA"), [1u8, 0])
            .sub(T(b"XLCN"), le(INN_LOC))
            .sub(T(b"XOWN"), le(GUARDS)),
        vec![
            Record::new(T(b"ACHR"), PLAYER)
                .zsub(EDID, "PlayerRef")
                .sub(T(b"NAME"), le(PLAYER_BASE))
                .sub(T(b"DATA"), zero.clone()),
            Record::new(T(b"ACHR"), GUARD)
                .zsub(EDID, "GuardRef")
                .sub(T(b"NAME"), le(GUARD_BASE))
                .sub(T(b"DATA"), zero.clone()),
            Record::new(T(b"REFR"), MARKER)
                .zsub(EDID, "MarkerRef")
                .sub(
                    T(b"VMAD"),
                    Vmad {
                        scripts: vec![ScriptEntry::new("ListenerScript")],
                        ..Default::default()
                    }
                    .write(T(b"REFR")),
                )
                .sub(T(b"NAME"), le(MARKER_BASE))
                .sub(T(b"XOWN"), le(PLAYER_BASE))
                .sub(T(b"DATA"), zero.clone()),
            Record::new(T(b"REFR"), GOLD_REF)
                .zsub(EDID, "GoldReference")
                .sub(T(b"NAME"), le(GOLD))
                .sub(T(b"DATA"), zero.clone()),
            Record::new(T(b"REFR"), OTHER_GOLD_REF)
                .zsub(EDID, "OtherGoldReference")
                .sub(T(b"NAME"), le(GOLD))
                .sub(T(b"DATA"), zero.clone()),
            Record::new(T(b"REFR"), CHEST)
                .zsub(EDID, "QueryChestReference")
                .sub(T(b"NAME"), le(CHEST_BASE))
                .sub(T(b"DATA"), zero.clone()),
        ],
    );
    let other = interior(
        &mut p,
        Record::new(T(b"CELL"), OTHER_CELL)
            .zsub(EDID, "OtherCell")
            .sub(T(b"DATA"), [1u8, 0])
            .sub(T(b"XLCN"), le(OTHER_TOWN_LOC)),
        Vec::new(),
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(inn));
    block.entries.push(Entry::Group(other));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

/// An event handler that traces a fixed message, or the parameter named
/// by `Err(index)`.
fn event_tracing(name: &str, params: &[(&str, &str)], message: Result<&str, usize>) -> Function {
    let mut f = FunctionBuilder::new(name, "None");
    for (p, t) in params {
        f = f.param(p, t);
    }
    let arg = match message {
        Ok(m) => s(m),
        Err(index) => ident(params[index].0),
    };
    f.ins(
        CallStatic,
        vec![s("Debug"), s("Trace"), ident("::nonevar"), arg, i(0)],
    )
    .build()
}

/// The synthetic stand-ins for `Alias.pex` and `ReferenceAlias.pex`: each
/// redeclares a `Form` native the way the vanilla files do, so a call from
/// a script extending them has to walk past the declaring class.
fn scripts() -> Vec<Vec<u8>> {
    let alias = ScriptBuilder::new("Alias", "Form")
        .function(
            FunctionBuilder::new("RegisterForSingleUpdate", "None")
                .param("afInterval", "Float")
                .native()
                .build(),
        )
        .bytes();
    let reference_alias = ScriptBuilder::new("ReferenceAlias", "Alias")
        .function(
            FunctionBuilder::new("GetReference", "ObjectReference")
                .native()
                .build(),
        )
        .bytes();
    let watcher = ScriptBuilder::new("WatcherScript", "ReferenceAlias")
        .function(
            FunctionBuilder::new("OnInit", "None")
                .local("r", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![s("GetReference"), ident("self"), ident("r")],
                )
                .ins(
                    CallStatic,
                    vec![s("Debug"), s("Trace"), ident("::nonevar"), ident("r"), i(0)],
                )
                .ins(
                    CallMethod,
                    vec![
                        s("RegisterForSingleUpdate"),
                        ident("self"),
                        ident("::nonevar"),
                        f(1.0),
                    ],
                )
                .build(),
        )
        .function(event_tracing("OnUpdate", &[], Ok("watcher update")))
        .bytes();
    let listener = ScriptBuilder::new("ListenerScript", "ObjectReference")
        .function(event_tracing(
            "OnAnimationEvent",
            &[("akSource", "ObjectReference"), ("asEventName", "String")],
            Err(1),
        ))
        .function(event_tracing(
            "OnTrackedStatsEvent",
            &[("asStat", "String"), ("aiValue", "Int")],
            Err(0),
        ))
        .function(event_tracing(
            "OnSleepStart",
            &[("afStart", "Float"), ("afEnd", "Float")],
            Ok("sleep start"),
        ))
        .function(event_tracing(
            "OnSleepStop",
            &[("abInterrupted", "Bool")],
            Ok("sleep stop"),
        ))
        .function(event_tracing(
            "OnGainLOS",
            &[("akViewer", "Actor"), ("akTarget", "ObjectReference")],
            Ok("los gained"),
        ))
        .bytes();
    let scene = ScriptBuilder::new("SceneScript", "Scene")
        .function(event_tracing("OnBegin", &[], Ok("scene begin")))
        .function(event_tracing("OnEnd", &[], Ok("scene end")))
        .bytes();
    vec![alias, reference_alias, watcher, listener, scene]
}

fn game() -> Game {
    let bytes = test_plugin().to_bytes();
    let plugin = Plugin::parse(&bytes).unwrap();
    let mut order = LoadOrder::new();
    order.add("Natives.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for s in scripts() {
        game.load_script(&s).unwrap();
    }
    game.load_cell(GlobalFormId {
        file: 0,
        object: INN_CELL,
    });
    game
}

/// CK `Game.ClearPrison` clears the prison variables on the player. The
/// current shared actor model represents that prison flag as `arrested`.
#[test]
fn scr_27_clear_prison_clears_player_arrest_state() {
    let mut game = game();
    let player = game.world_mut().player();
    game.world_mut().actor(player).unwrap().arrested = true;
    game.world_mut().actor(player).unwrap().days_in_jail = 4;

    call_global(&mut game, "Game", "ClearPrison", &[]);

    assert!(!game.world_mut().actor(player).unwrap().arrested);
    assert_eq!(game.world_mut().actor(player).unwrap().days_in_jail, 0);
    assert!(errors(&game).is_empty());
}

static PROBES: AtomicUsize = AtomicUsize::new(0);
static PROBE_SCRIPTS: Mutex<Vec<(usize, String, Vec<u8>)>> = Mutex::new(Vec::new());

fn remember_probe(game: &Game, name: &str, bytes: &[u8]) {
    PROBE_SCRIPTS.lock().unwrap().push((
        std::ptr::from_ref(game) as usize,
        name.to_string(),
        bytes.to_vec(),
    ));
}

fn copy_probe_scripts(from: &Game, to: &mut Game) {
    let from_id = std::ptr::from_ref(from) as usize;
    let loaded: std::collections::HashSet<_> = from
        .vm
        .loaded_script_fingerprints()
        .into_iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect();
    let scripts: Vec<_> = PROBE_SCRIPTS
        .lock()
        .unwrap()
        .iter()
        .filter(|(owner, name, _)| *owner == from_id && loaded.contains(&name.to_ascii_lowercase()))
        .map(|(_, _, bytes)| bytes.clone())
        .collect();
    for bytes in scripts {
        to.load_script(&bytes).unwrap();
    }
}

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
    let script = format!("Probe{n}");
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
    remember_probe(game, &script, &bytes);
    let mut all = vec![target.clone()];
    all.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &all).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

fn wait_for_animation_thread(
    game: &mut Game,
    target: &Value,
    event: &str,
) -> soe_papyrus::ThreadId {
    let n = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Probe{n}");
    let fun = FunctionBuilder::new("Run", "Bool")
        .global()
        .param("target", "ObjectReference")
        .param("event", "String")
        .local("result", "Bool")
        .ins(
            CallMethod,
            vec![
                s("WaitForAnimationEvent"),
                ident("target"),
                ident("result"),
                ident("event"),
            ],
        )
        .ins(Return, vec![ident("result")])
        .build();
    let bytes = ScriptBuilder::new(&script, "Form").function(fun).bytes();
    game.load_script(&bytes).unwrap();
    let thread = game
        .vm
        .call_global(&script, "Run", &[target.clone(), Value::string(event)])
        .unwrap();
    game.run_until_idle(64);
    thread
}

/// Call a global `Class.Name(args)` the same way.
fn call_global(game: &mut Game, class: &str, name: &str, args: &[Value]) -> Value {
    let n = PROBES.fetch_add(1, Ordering::SeqCst);
    let script = format!("Probe{n}");
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
    remember_probe(game, &script, &bytes);
    let thread = game.vm.call_global(&script, "Run", args).unwrap();
    game.run_until_idle(64);
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_21_set_god_mode_updates_the_shared_runtime_switch() {
    let mut game = game();

    assert!(matches!(
        call_global(&mut game, "Debug", "SetGodMode", &[Value::Bool(true)]),
        Value::None
    ));
    assert!(game.world().debug_god_mode);
    assert!(matches!(
        call_global(&mut game, "Debug", "SetGodMode", &[Value::Bool(false)]),
        Value::None
    ));
    assert!(!game.world().debug_god_mode);
}

#[test]
fn scr_21_toggle_ai_alternates_the_package_driver_switch() {
    let mut game = game();
    assert!(game.world().debug_ai_enabled);

    call_global(&mut game, "Debug", "ToggleAI", &[]);
    assert!(!game.world().debug_ai_enabled);
    call_global(&mut game, "Debug", "ToggleAI", &[]);
    assert!(game.world().debug_ai_enabled);
}

#[test]
fn scr_21_toggle_collisions_alternates_the_shared_runtime_switch() {
    let mut game = game();
    assert!(game.world().debug_collisions_enabled);

    call_global(&mut game, "Debug", "ToggleCollisions", &[]);
    assert!(!game.world().debug_collisions_enabled);
    call_global(&mut game, "Debug", "ToggleCollisions", &[]);
    assert!(game.world().debug_collisions_enabled);
}

#[test]
fn scr_21_dump_alias_data_reports_a_non_quest_argument() {
    let mut game = game();

    assert!(matches!(
        call_global(&mut game, "Debug", "DumpAliasData", &[Value::None]),
        Value::None
    ));
    assert!(game
        .vm
        .log
        .contains("Debug.DumpAliasData: argument is not a quest"));
}

#[test]
fn scr_21_dump_alias_data_logs_the_quest_alias_fill() {
    let mut game = game();
    let quest_id = game.world().order().resolve(0, DEBUG_QUEST);
    let quest = Value::Object(game.object_of_form(quest_id));
    let alias_found = game
        .world_mut()
        .quest_state(quest_id)
        .is_some_and(|state| state.aliases.contains_key(&7));
    assert!(
        alias_found,
        "quest record: {:?}",
        game.world().record(quest_id)
    );

    assert!(matches!(
        call_global(&mut game, "Debug", "DumpAliasData", &[quest]),
        Value::None
    ));
    assert!(
        game.vm.log.contains("Debug.DumpAliasData: 7 empty"),
        "{:?}",
        game.vm.log.messages()
    );
}

#[test]
fn scr_21_debug_channel_notify_logs_channel_and_message() {
    let mut game = game();

    assert!(matches!(
        call_global(
            &mut game,
            "Debug",
            "DebugChannelNotify",
            &[Value::string("combat"), Value::string("hit")],
        ),
        Value::None
    ));
    assert!(game.vm.log.contains("[combat] hit"));
}

fn obj(game: &mut Game, edid: &str) -> Value {
    let id = game.form(edid).unwrap_or_else(|| panic!("no form {edid}"));
    Value::Object(game.object_of_form(id))
}

fn is(game: &mut Game, v: &Value, edid: &str) -> bool {
    let id = game.form(edid).unwrap();
    v.as_object() == Some(game.object_of_form(id))
}

fn errors(game: &Game) -> Vec<String> {
    game.traces()
        .into_iter()
        .filter(|t| t.contains("no function") || t.contains("no global") || t.contains("error"))
        .collect()
}

/// CK GetItemCount returns the exact base total, sums every FormList member,
/// selects all inventory bases with a keyword and selects one concrete
/// reference through ExtraOriginalReference. The engine supplies keyword
/// counts for actors and containers despite the CK page's actor caveat.
#[test]
fn get_item_count_queries_base_lists_keywords_and_references() {
    let mut g = game();
    let guard_id = g.form("GuardRef").unwrap();
    let gold_id = g.form("Gold001").unwrap();
    let sword_id = g.form("IronSword").unwrap();
    let gold_reference_id = g.form("GoldReference").unwrap();
    let other_gold_reference_id = g.form("OtherGoldReference").unwrap();
    let chest_id = g.form("QueryChestReference").unwrap();
    {
        let mut world = g.world_mut();
        let inventory = &mut world.reference(guard_id).unwrap().inventory;
        inventory
            .restore(
                gold_id,
                4,
                vec![
                    Stack::new(
                        1,
                        vec![Extra {
                            kind: 32,
                            fields: vec![Field::Form(Some(gold_reference_id))],
                        }],
                    )
                    .unwrap(),
                    Stack::new(
                        5,
                        vec![Extra {
                            kind: 32,
                            fields: vec![Field::Form(Some(gold_reference_id))],
                        }],
                    )
                    .unwrap(),
                    Stack::new(
                        1,
                        vec![Extra {
                            kind: 32,
                            fields: vec![Field::Form(Some(other_gold_reference_id))],
                        }],
                    )
                    .unwrap(),
                    Stack::new(
                        -2,
                        vec![
                            Extra {
                                kind: 32,
                                fields: vec![Field::Form(Some(gold_reference_id))],
                            },
                            Extra {
                                kind: 46,
                                fields: Vec::new(),
                            },
                        ],
                    )
                    .unwrap(),
                ],
            )
            .unwrap();
        inventory.add(sword_id, 2).unwrap();
        world
            .reference(chest_id)
            .unwrap()
            .inventory
            .add(sword_id, 5)
            .unwrap();
    }

    let guard = obj(&mut g, "GuardRef");
    let gold = obj(&mut g, "Gold001");
    let list = obj(&mut g, "CarriedItems");
    let keyword = obj(&mut g, "WeaponTypeTest");
    let gold_reference = obj(&mut g, "GoldReference");
    let other_gold_reference = obj(&mut g, "OtherGoldReference");
    let cyclic = obj(&mut g, "CyclicItems");
    let chest = obj(&mut g, "QueryChestReference");
    assert_eq!(call(&mut g, &guard, "GetItemCount", &[gold]), Value::Int(4));
    assert_eq!(call(&mut g, &guard, "GetItemCount", &[list]), Value::Int(6));
    assert_eq!(
        call(&mut g, &guard, "GetItemCount", &[keyword.clone()]),
        Value::Int(2)
    );
    assert_eq!(
        call(&mut g, &guard, "GetItemCount", &[gold_reference]),
        Value::Int(4)
    );
    assert_eq!(
        call(&mut g, &guard, "GetItemCount", &[other_gold_reference]),
        Value::Int(1)
    );
    assert_eq!(
        call(&mut g, &guard, "GetItemCount", &[cyclic]),
        Value::Int(0)
    );
    assert_eq!(
        call(&mut g, &chest, "GetItemCount", &[keyword]),
        Value::Int(5)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn get_item_count_tracks_reference_pickup_transfer_save_and_consumption() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let chest = obj(&mut g, "QueryChestReference");
    let gold = obj(&mut g, "Gold001");
    let reference = obj(&mut g, "GoldReference");
    let other_reference = obj(&mut g, "OtherGoldReference");
    call(&mut g, &guard, "AddItem", &[gold.clone(), Value::Int(5)]);
    for stage in 0..4 {
        match stage {
            0 => {
                call(
                    &mut g,
                    &guard,
                    "AddItem",
                    &[reference.clone(), Value::Int(1)],
                );
            }
            1 => {
                call(
                    &mut g,
                    &chest,
                    "AddItem",
                    &[reference.clone(), Value::Int(1)],
                );
            }
            2 => {
                let bytes = g.world().save_state();
                g.world_mut().load_state(&bytes).unwrap();
            }
            _ => {
                call(&mut g, &chest, "RemoveItem", &[gold.clone(), Value::Int(1)]);
            }
        }
        for (holder, base_count, reference_count) in [
            (
                &guard,
                if stage == 0 { 6 } else { 5 },
                i32::from(stage == 0),
            ),
            (
                &chest,
                i32::from(stage == 1 || stage == 2),
                i32::from(stage == 1 || stage == 2),
            ),
        ] {
            for (query, expected) in [
                (&gold, base_count),
                (&reference, reference_count),
                (&other_reference, 0),
            ] {
                assert_eq!(
                    call(&mut g, holder, "GetItemCount", std::slice::from_ref(query)),
                    Value::Int(expected)
                );
            }
        }
    }
    call(
        &mut g,
        &guard,
        "AddItem",
        &[reference.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &guard, "GetItemCount", &[reference]),
        Value::Int(0)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- leveled lists -----

/// AddForm "Adds a form to this leveled list", GetNumForms and the Nth
/// getters read entries in order, Revert "reverts the list back to its
/// original state" and drops what AddForm added.
#[test]
fn a_leveled_item_list_takes_forms_and_reverts() {
    let mut g = game();
    let list = obj(&mut g, "LootList");
    assert_eq!(call(&mut g, &list, "GetNumForms", &[]), Value::Int(2));
    assert_eq!(call(&mut g, &list, "GetChanceNone", &[]), Value::Int(25));
    assert_eq!(
        call(&mut g, &list, "GetNthLevel", &[Value::Int(1)]),
        Value::Int(10)
    );
    assert_eq!(
        call(&mut g, &list, "GetNthCount", &[Value::Int(0)]),
        Value::Int(5)
    );
    let first = call(&mut g, &list, "GetNthForm", &[Value::Int(0)]);
    assert!(is(&mut g, &first, "Gold001"));
    let flames = obj(&mut g, "Flames");
    call(
        &mut g,
        &list,
        "AddForm",
        &[flames, Value::Int(20), Value::Int(3)],
    );
    assert_eq!(call(&mut g, &list, "GetNumForms", &[]), Value::Int(3));
    assert_eq!(
        call(&mut g, &list, "GetNthLevel", &[Value::Int(2)]),
        Value::Int(20)
    );
    assert_eq!(
        call(&mut g, &list, "GetNthCount", &[Value::Int(2)]),
        Value::Int(3)
    );
    call(&mut g, &list, "SetChanceNone", &[Value::Int(50)]);
    assert_eq!(call(&mut g, &list, "GetChanceNone", &[]), Value::Int(50));
    call(&mut g, &list, "Revert", &[]);
    assert_eq!(call(&mut g, &list, "GetNumForms", &[]), Value::Int(2));
    assert_eq!(call(&mut g, &list, "GetChanceNone", &[]), Value::Int(25));
    assert_eq!(
        call(&mut g, &list, "GetNthForm", &[Value::Int(5)]),
        Value::None
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// LeveledActor.AddForm takes an actor base with level and count,
/// LeveledSpell.AddForm takes a spell and a level only.
#[test]
fn leveled_actor_and_spell_lists_share_the_shape() {
    let mut g = game();
    let enemies = obj(&mut g, "EnemyList");
    let base = obj(&mut g, "GuardBase");
    call(
        &mut g,
        &enemies,
        "AddForm",
        &[base, Value::Int(12), Value::Int(2)],
    );
    assert_eq!(call(&mut g, &enemies, "GetNumForms", &[]), Value::Int(2));
    assert_eq!(
        call(&mut g, &enemies, "GetNthLevel", &[Value::Int(0)]),
        Value::Int(5)
    );
    let spells = obj(&mut g, "SpellList");
    let flames = obj(&mut g, "Flames");
    call(&mut g, &spells, "AddForm", &[flames, Value::Int(30)]);
    assert_eq!(call(&mut g, &spells, "GetNumForms", &[]), Value::Int(2));
    assert!(!g.vm.natives.has_method("LeveledSpell", "GetNthCount"));
    call(&mut g, &spells, "Revert", &[]);
    assert_eq!(call(&mut g, &spells, "GetNumForms", &[]), Value::Int(1));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn scr_64_leveled_actor_and_item_bindings_match_skse_contracts() {
    let mut g = game();
    let items = obj(&mut g, "LootList");
    let actors = obj(&mut g, "EnemyList");

    let actor_base = obj(&mut g, "GuardBase");
    call(&mut g, &actors, "AddForm", &[actor_base, Value::Int(24)]);
    assert_eq!(call(&mut g, &actors, "GetNumForms", &[]), Value::Int(2));
    assert_eq!(
        call(&mut g, &actors, "GetNthCount", &[Value::Int(1)]),
        Value::Int(1)
    );
    for list in [&items, &actors] {
        assert_eq!(call(&mut g, list, "GetNumForms", &[]), Value::Int(2));
    }
    call(&mut g, &items, "SetChanceNone", &[Value::Int(42)]);
    assert_eq!(call(&mut g, &items, "GetChanceNone", &[]), Value::Int(42));

    for list in [&items, &actors] {
        assert_ne!(
            call(&mut g, list, "GetNthForm", &[Value::Int(0)]),
            Value::None
        );
        call(
            &mut g,
            list,
            "SetNthLevel",
            &[Value::Int(0), Value::Int(70_000)],
        );
        call(
            &mut g,
            list,
            "SetNthCount",
            &[Value::Int(0), Value::Int(70_000)],
        );
        assert_eq!(
            call(&mut g, list, "GetNthLevel", &[Value::Int(0)]),
            Value::Int(i32::from(u16::MAX))
        );
        assert_eq!(
            call(&mut g, list, "GetNthCount", &[Value::Int(0)]),
            Value::Int(i32::from(u16::MAX))
        );
        call(
            &mut g,
            list,
            "SetNthLevel",
            &[Value::Int(99), Value::Int(12)],
        );
        call(
            &mut g,
            list,
            "SetNthCount",
            &[Value::Int(99), Value::Int(12)],
        );
        assert_eq!(
            call(&mut g, list, "GetNthLevel", &[Value::Int(0)]),
            Value::Int(i32::from(u16::MAX))
        );
        assert_eq!(
            call(&mut g, list, "GetNthCount", &[Value::Int(0)]),
            Value::Int(i32::from(u16::MAX))
        );
    }
    assert_eq!(call(&mut g, &actors, "GetChanceNone", &[]), Value::Int(0));
    assert_eq!(call(&mut g, &items, "GetChanceGlobal", &[]), Value::None);
    let chance_global = obj(&mut g, "Counter");
    assert_eq!(
        call(&mut g, &items, "SetChanceGlobal", &[chance_global]),
        Value::None
    );
    let binding_errors = errors(&g);
    assert!(
        binding_errors
            .iter()
            .any(|trace| trace.contains("no function")),
        "{binding_errors:?}"
    );
}

/// The method inventory follows the shipped Location script and SKSE's
/// vanilla and modified LeveledSpell scripts.
#[test]
fn scr_65_registry_inventory_matches_location_and_leveled_spell_contracts() {
    let g = game();
    let mut expected = Vec::new();
    for class in ["LeveledItem", "LeveledActor"] {
        for name in [
            "AddForm",
            "Revert",
            "GetNumForms",
            "GetNthForm",
            "GetNthLevel",
            "SetNthLevel",
            "GetNthCount",
            "GetChanceNone",
            "SetChanceNone",
        ] {
            expected.push((class, name));
        }
    }
    for name in [
        "AddForm",
        "Revert",
        "GetNumForms",
        "GetNthForm",
        "GetNthLevel",
        "SetNthLevel",
        "GetChanceNone",
        "SetChanceNone",
    ] {
        expected.push(("LeveledSpell", name));
    }
    for name in [
        "GetKeywordData",
        "SetKeywordData",
        "IsCleared",
        "SetCleared",
        "IsChild",
        "HasCommonParent",
        "IsSameLocation",
        "IsLoaded",
        "HasRefType",
        "GetRefTypeAliveCount",
        "GetRefTypeDeadCount",
    ] {
        expected.push(("Location", name));
    }

    let inventory: Vec<_> = expected
        .iter()
        .copied()
        .filter(|(class, name)| g.vm.natives.has_method(class, name))
        .collect();
    assert_eq!(inventory, expected);
    assert!(expected
        .iter()
        .all(|(class, name)| !g.vm.natives.has_global(class, name)));
    assert!(!g.vm.natives.has_method("LeveledSpell", "GetNthCount"));
}

/// SKSE exposes SetNthLevel on LeveledSpell, clamps large levels to u16,
/// and leaves the list unchanged for an out-of-range entry index.
#[test]
fn scr_65_leveled_spell_setters_match_skse_contract() {
    let mut g = game();
    let spells = obj(&mut g, "SpellList");
    call(
        &mut g,
        &spells,
        "SetNthLevel",
        &[Value::Int(0), Value::Int(70_000)],
    );
    assert_eq!(
        call(&mut g, &spells, "GetNthLevel", &[Value::Int(0)]),
        Value::Int(i32::from(u16::MAX))
    );
    call(
        &mut g,
        &spells,
        "SetNthLevel",
        &[Value::Int(4), Value::Int(12)],
    );
    assert_eq!(
        call(&mut g, &spells, "GetNthLevel", &[Value::Int(0)]),
        Value::Int(i32::from(u16::MAX))
    );
    call(&mut g, &spells, "SetChanceNone", &[Value::Int(101)]);
    assert_eq!(call(&mut g, &spells, "GetChanceNone", &[]), Value::Int(101));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- locations -----

/// Location.IsSameLocation accepts identity, a matching shared ancestor,
/// and either direction of a keyworded parent/child relationship.
#[test]
fn scr_65_same_location_matches_keyworded_parent_child_contract() {
    let mut g = game();
    let hold = obj(&mut g, "HoldLoc");
    let town = obj(&mut g, "TownLoc");
    let inn = obj(&mut g, "InnLoc");
    let other = obj(&mut g, "OtherTownLoc");
    let keyword = obj(&mut g, "LocTypeHold");

    assert_eq!(
        call(
            &mut g,
            &hold,
            "IsSameLocation",
            &[town.clone(), keyword.clone()]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            &mut g,
            &town,
            "IsSameLocation",
            &[hold.clone(), keyword.clone()]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &inn, "IsSameLocation", &[other, keyword]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &inn, "IsSameLocation", &[hold, Value::None]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SetKeywordData stores a float per keyword on the location and
/// GetKeywordData reads it back, zero when never set.
#[test]
fn location_keyword_data_round_trips() {
    let mut g = game();
    let hold = obj(&mut g, "HoldLoc");
    let kw = obj(&mut g, "LocTypeHold");
    assert_eq!(
        call(&mut g, &hold, "GetKeywordData", &[kw.clone()]),
        Value::Float(0.0)
    );
    call(
        &mut g,
        &hold,
        "SetKeywordData",
        &[kw.clone(), Value::Float(3.5)],
    );
    assert_eq!(
        call(&mut g, &hold, "GetKeywordData", &[kw]),
        Value::Float(3.5)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// IsChild "whether the passed in location is a child of this one",
/// HasCommonParent finds a shared ancestor, optionally one carrying the
/// filter keyword, IsSameLocation is identity or a filtered shared
/// parent, SetCleared and IsCleared are a flag.
#[test]
fn locations_know_their_parents() {
    let mut g = game();
    let hold = obj(&mut g, "HoldLoc");
    let town = obj(&mut g, "TownLoc");
    let inn = obj(&mut g, "InnLoc");
    let other = obj(&mut g, "OtherTownLoc");
    let kw = obj(&mut g, "LocTypeHold");
    assert_eq!(
        call(&mut g, &hold, "IsChild", &[inn.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &town, "IsChild", &[inn.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &inn, "IsChild", &[hold.clone()]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &inn, "IsChild", &[inn.clone()]),
        Value::Bool(false)
    );
    assert_eq!(
        call(
            &mut g,
            &inn,
            "HasCommonParent",
            &[other.clone(), Value::None]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            &mut g,
            &inn,
            "HasCommonParent",
            &[other.clone(), kw.clone()]
        ),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &inn, "HasCommonParent", &[hold.clone(), kw.clone()]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &inn, "IsSameLocation", &[inn.clone(), Value::None]),
        Value::Bool(true)
    );
    assert_eq!(
        call(
            &mut g,
            &inn,
            "IsSameLocation",
            &[other.clone(), Value::None]
        ),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &inn, "IsSameLocation", &[other, kw]),
        Value::Bool(true)
    );
    assert_eq!(call(&mut g, &inn, "IsCleared", &[]), Value::Bool(false));
    call(&mut g, &inn, "SetCleared", &[]);
    assert_eq!(call(&mut g, &inn, "IsCleared", &[]), Value::Bool(true));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Location.IsLoaded follows the active cell's XLCN, including its parent
/// locations, and stays false for an unloaded or invalid location.
#[test]
fn scr_35_location_is_loaded_tracks_active_cell() {
    let mut g = game();
    let other_cell = g.form("OtherCell").unwrap();
    g.world_mut().load_cell(other_cell);
    let inn = obj(&mut g, "InnLoc");
    let town = obj(&mut g, "TownLoc");
    let hold = obj(&mut g, "HoldLoc");
    let other = obj(&mut g, "OtherTownLoc");

    assert_eq!(call(&mut g, &inn, "IsLoaded", &[]), Value::Bool(false));
    let cell = g.form("InnCell").unwrap();
    g.world_mut().load_cell(cell);
    assert_eq!(call(&mut g, &inn, "IsLoaded", &[]), Value::Bool(true));
    assert_eq!(call(&mut g, &town, "IsLoaded", &[]), Value::Bool(true));
    assert_eq!(call(&mut g, &hold, "IsLoaded", &[]), Value::Bool(true));
    assert_eq!(call(&mut g, &other, "IsLoaded", &[]), Value::Bool(false));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Location.HasRefType reads the location's ACSR and LCSR entries and
/// returns false for a different form type, absent type or wrong argument.
#[test]
fn scr_35_location_has_ref_type_reads_authored_entries() {
    let mut g = game();
    let inn = obj(&mut g, "InnLoc");
    let ref_type = obj(&mut g, "GuardRefType");
    let absent = obj(&mut g, "LocTypeHold");

    assert_eq!(
        call(&mut g, &inn, "HasRefType", &[ref_type]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &inn, "HasRefType", &[absent.clone()]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &inn, "HasRefType", &[Value::None]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &inn, "HasRefType", &[absent]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// The count methods count linked ACHR references by LCRT and observe the
/// runtime actor death flag. Invalid and unrelated form arguments return 0.
#[test]
fn scr_35_location_ref_type_counts_follow_actor_state() {
    let mut g = game();
    let inn = obj(&mut g, "InnLoc");
    let ref_type = obj(&mut g, "GuardRefType");
    let keyword = obj(&mut g, "LocTypeHold");

    assert_eq!(
        call(&mut g, &inn, "GetRefTypeAliveCount", &[ref_type.clone()]),
        Value::Int(1)
    );
    assert_eq!(
        call(&mut g, &inn, "GetRefTypeDeadCount", &[ref_type.clone()]),
        Value::Int(0)
    );
    let guard = g.form("GuardRef").unwrap();
    g.world_mut().actor(guard).unwrap().dead = true;
    assert_eq!(
        call(&mut g, &inn, "GetRefTypeAliveCount", &[ref_type.clone()]),
        Value::Int(0)
    );
    assert_eq!(
        call(&mut g, &inn, "GetRefTypeDeadCount", &[ref_type]),
        Value::Int(1)
    );
    assert_eq!(
        call(&mut g, &inn, "GetRefTypeAliveCount", &[keyword.clone()]),
        Value::Int(0)
    );
    assert_eq!(
        call(&mut g, &inn, "GetRefTypeDeadCount", &[Value::None]),
        Value::Int(0)
    );
    assert_eq!(
        call(&mut g, &inn, "GetRefTypeAliveCount", &[keyword]),
        Value::Int(0)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// GetCurrentLocation is the cell's location, GetEditorLocation "obtains
/// this reference's editor Location" and does not follow a MoveTo,
/// IsInLocation "checks to see if this object is in the specified
/// location, or in a child of the specified location".
#[test]
fn references_report_their_location() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let loc = call(&mut g, &guard, "GetCurrentLocation", &[]);
    assert!(is(&mut g, &loc, "InnLoc"));
    let inn = obj(&mut g, "InnLoc");
    let hold = obj(&mut g, "HoldLoc");
    let other = obj(&mut g, "OtherTownLoc");
    assert_eq!(
        call(&mut g, &guard, "IsInLocation", &[inn]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &guard, "IsInLocation", &[hold]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &guard, "IsInLocation", &[other.clone()]),
        Value::Bool(false)
    );
    let guard_id = g.form("GuardRef").unwrap();
    let other_cell = g.form("OtherCell").unwrap();
    g.world_mut().reference(guard_id).unwrap().cell = Some(other_cell);
    let now = call(&mut g, &guard, "GetCurrentLocation", &[]);
    assert!(is(&mut g, &now, "OtherTownLoc"));
    let editor = call(&mut g, &guard, "GetEditorLocation", &[]);
    assert!(is(&mut g, &editor, "InnLoc"));
    assert_eq!(
        call(&mut g, &guard, "MoveToMyEditorLocation", &[]),
        Value::Bool(true)
    );
    let back = call(&mut g, &guard, "GetCurrentLocation", &[]);
    assert!(is(&mut g, &back, "InnLoc"));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// CK `MoveTo_-_ObjectReference.wikitext` defines the rotation default and
/// the opt-out argument.
#[test]
fn scr_67_move_to_respects_match_rotation_and_default() {
    let mut g = game();
    g.start();
    let mover_id = g.form("GoldReference").unwrap();
    let target_id = g.form("OtherGoldReference").unwrap();
    let mover = obj(&mut g, "GoldReference");
    let target = obj(&mut g, "OtherGoldReference");
    g.world_mut().reference(mover_id).unwrap().rotation = [0.1, 0.2, 0.3];
    g.world_mut().reference(target_id).unwrap().rotation = [0.4, 0.5, 0.6];

    call(&mut g, &mover, "MoveTo", &[target.clone()]);
    assert_eq!(
        g.world_mut().reference(mover_id).unwrap().rotation,
        [0.4, 0.5, 0.6]
    );

    g.world_mut().reference(mover_id).unwrap().rotation = [0.1, 0.2, 0.3];
    call(
        &mut g,
        &mover,
        "MoveTo",
        &[
            target,
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Bool(false),
        ],
    );
    assert_eq!(
        g.world_mut().reference(mover_id).unwrap().rotation,
        [0.1, 0.2, 0.3]
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// GetLocation on a location alias, ForceLocationTo fills it, Clear
/// (declared on Alias) empties it.
#[test]
fn scr_66_location_alias_holds_a_location() {
    let mut g = game();
    g.start();
    let quest = g.form("LocQuest").unwrap();
    let alias = Value::Object(g.object_of(Handle::Alias { quest, id: 0 }));
    assert_eq!(call(&mut g, &alias, "GetLocation", &[]), Value::None);
    let inn = obj(&mut g, "InnLoc");
    call(&mut g, &alias, "ForceLocationTo", &[inn]);
    let got = call(&mut g, &alias, "GetLocation", &[]);
    assert!(is(&mut g, &got, "InnLoc"));
    call(&mut g, &alias, "Clear", &[]);
    assert_eq!(call(&mut g, &alias, "GetLocation", &[]), Value::None);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn alias_getters_preserve_reference_and_actor_return_types() {
    let mut g = game();
    g.start();
    let quest = g.form("LocQuest").unwrap();
    let alias = Value::Object(g.object_of(Handle::Alias { quest, id: 1 }));
    for (name, reference, actor) in [
        ("GuardRef", true, true),
        ("MarkerRef", true, false),
        ("InnLoc", false, false),
    ] {
        let target = g.form(name).unwrap();
        assert!(g.world_mut().force_alias(quest, 1, Some(target)));
        let object = obj(&mut g, name);
        for (getter, allowed) in [
            ("GetReference", reference),
            ("GetRef", reference),
            ("GetActorReference", actor),
            ("GetActorRef", actor),
        ] {
            let expected = if allowed { object.clone() } else { Value::None };
            assert_eq!(
                call(&mut g, &alias, getter, &[]),
                expected,
                "{getter}: {name}"
            );
        }
    }
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// This checks the 21 registered Form methods against CK Form_Script. SKSE's
/// GetType and GetName appear in its Form declaration. GetKeywords is an
/// engine extension.
#[test]
fn scr_59_form_registry_matches_the_contract_inventory() {
    let g = game();
    let bindings = [
        "GetFormID",
        "GetName",
        "GetType",
        "HasKeyword",
        "GetKeywords",
        "RegisterForSingleUpdate",
        "RegisterForUpdate",
        "RegisterForSingleUpdateGameTime",
        "RegisterForUpdateGameTime",
        "UnregisterForUpdate",
        "UnregisterForUpdateGameTime",
        "RegisterForSleep",
        "UnregisterForSleep",
        "RegisterForTrackedStatsEvent",
        "UnregisterForTrackedStatsEvent",
        "RegisterForAnimationEvent",
        "UnregisterForAnimationEvent",
        "RegisterForLOS",
        "RegisterForSingleLOSGain",
        "RegisterForSingleLOSLost",
        "UnregisterForLOS",
    ];
    for binding in bindings {
        assert!(
            g.vm.natives.has_method("Form", binding),
            "Form.{binding} is missing from the native registry"
        );
    }
}

/// CK declares ForceRefTo(ObjectReference). Other form types leave the alias
/// unchanged, while a reference replaces its current target.
#[test]
fn scr_59_force_ref_to_accepts_only_object_references() {
    let mut g = game();
    g.start();
    let quest = g.form("LocQuest").unwrap();
    let alias = Value::Object(g.object_of(Handle::Alias { quest, id: 1 }));
    let original = g.world_mut().alias_ref(quest, 1);

    let location = obj(&mut g, "InnLoc");
    call(&mut g, &alias, "ForceRefTo", &[location]);
    assert_eq!(g.world_mut().alias_ref(quest, 1), original);

    let reference = obj(&mut g, "MarkerRef");
    let expected = g.world().form("MarkerRef").unwrap();
    call(&mut g, &alias, "ForceRefTo", &[reference]);
    assert_eq!(g.world_mut().alias_ref(quest, 1), Some(expected));

    let actor = obj(&mut g, "GuardRef");
    let expected_actor = g.world().form("GuardRef").unwrap();
    call(&mut g, &alias, "ForceRefTo", &[actor]);
    assert_eq!(g.world_mut().alias_ref(quest, 1), Some(expected_actor));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- factions -----

/// GetReaction reads the record's XNAM (2 is ally), SetEnemy "sets the
/// two factions as enemies (or neutral)" in both directions, SetAlly
/// "sets the two factions as allies (or friends)".
#[test]
fn faction_relations_write_both_directions() {
    let mut g = game();
    let guards = obj(&mut g, "GuardsFaction");
    let thieves = obj(&mut g, "ThievesFaction");
    assert_eq!(
        call(&mut g, &guards, "GetReaction", &[thieves.clone()]),
        Value::Int(2)
    );
    assert_eq!(
        call(&mut g, &thieves, "GetReaction", &[guards.clone()]),
        Value::Int(0)
    );
    call(
        &mut g,
        &guards,
        "SetEnemy",
        &[thieves.clone(), Value::Bool(false), Value::Bool(true)],
    );
    assert_eq!(
        call(&mut g, &guards, "GetReaction", &[thieves.clone()]),
        Value::Int(1)
    );
    assert_eq!(
        call(&mut g, &thieves, "GetReaction", &[guards.clone()]),
        Value::Int(0)
    );
    call(
        &mut g,
        &guards,
        "SetAlly",
        &[thieves.clone(), Value::Bool(true), Value::Bool(false)],
    );
    assert_eq!(
        call(&mut g, &guards, "GetReaction", &[thieves.clone()]),
        Value::Int(3)
    );
    assert_eq!(
        call(&mut g, &thieves, "GetReaction", &[guards.clone()]),
        Value::Int(2)
    );
    call(
        &mut g,
        &guards,
        "SetReaction",
        &[thieves.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &guards, "GetReaction", &[thieves]),
        Value::Int(1)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Faction.IsFactionInCrimeGroup checks its argument against FACT.CRGR's
/// FLST members, independent of the faction's Track Crime flag.
#[test]
fn scr_58_faction_crime_group_uses_the_other_faction_argument() {
    let mut g = game();
    let guards_id = g.form("GuardsFaction").unwrap();
    let thieves_id = g.form("ThievesFaction").unwrap();
    let group_id = soe_esm::GlobalFormId {
        file: guards_id.file,
        object: SHARED_CRIME_GROUP,
    };
    assert_eq!(
        g.world()
            .record(guards_id)
            .and_then(|record| record.find(T(b"CRGR")))
            .and_then(|bytes| (bytes.len() >= 4)
                .then(|| u32::from_le_bytes(bytes[..4].try_into().unwrap()))),
        Some(SHARED_CRIME_GROUP)
    );
    assert_eq!(
        g.world().resolve_from(guards_id, SHARED_CRIME_GROUP),
        Some(group_id)
    );
    assert_eq!(
        g.world_mut().form_list(group_id).unwrap().as_slice(),
        &[thieves_id]
    );
    let guards = obj(&mut g, "GuardsFaction");
    let thieves = obj(&mut g, "ThievesFaction");
    let marker = obj(&mut g, "MarkerRef");

    assert_eq!(
        call(&mut g, &guards, "IsFactionInCrimeGroup", &[thieves.clone()]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &guards, "IsFactionInCrimeGroup", &[guards.clone()]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &guards, "IsFactionInCrimeGroup", &[marker]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &thieves, "IsFactionInCrimeGroup", &[guards]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// `ModReaction` applies its amount to the selected direction only.
#[test]
fn scr_25_mod_reaction_changes_selected_direction() {
    let mut g = game();
    let guards = obj(&mut g, "GuardsFaction");
    let thieves = obj(&mut g, "ThievesFaction");
    call(
        &mut g,
        &guards,
        "ModReaction",
        &[thieves.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &guards, "GetReaction", &[thieves.clone()]),
        Value::Int(3),
        "the requested amount must be applied"
    );
    assert_eq!(
        call(&mut g, &thieves, "GetReaction", &[guards]),
        Value::Int(0),
        "the reverse relation must not change"
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// `ModReaction` defaults a missing relation to neutral, ignores a missing
/// target, and saturates instead of overflowing.
#[test]
fn scr_25_mod_reaction_handles_default_missing_target_and_overflow() {
    let mut g = game();
    let guards_id = g.form("GuardsFaction").unwrap();
    let thieves_id = g.form("ThievesFaction").unwrap();
    let guards = obj(&mut g, "GuardsFaction");
    let thieves = obj(&mut g, "ThievesFaction");
    call(
        &mut g,
        &thieves,
        "ModReaction",
        &[guards.clone(), Value::Int(-1)],
    );
    assert_eq!(
        call(&mut g, &thieves, "GetReaction", &[guards.clone()]),
        Value::Int(-1),
        "an absent relation starts from neutral"
    );
    call(
        &mut g,
        &thieves,
        "ModReaction",
        &[Value::None, Value::Int(8)],
    );
    assert_eq!(
        g.world_mut()
            .faction(thieves_id)
            .unwrap()
            .reactions
            .get(&guards_id),
        Some(&-1),
        "a missing target must leave the existing relation unchanged"
    );
    g.world_mut()
        .faction(guards_id)
        .unwrap()
        .reactions
        .insert(thieves_id, i32::MAX);
    call(
        &mut g,
        &guards,
        "ModReaction",
        &[thieves.clone(), Value::Int(1)],
    );
    assert_eq!(
        call(&mut g, &guards, "GetReaction", &[thieves]),
        Value::Int(i32::MAX),
        "the reaction must saturate at the integer boundary"
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// ModCrimeGold adds to the violent or non-violent bounty and to the
/// infamy of that kind, GetCrimeGold is both, CanPayCrimeGold checks the
/// player's gold, PlayerPayCrimeGold takes it and clears the bounty,
/// expelled and player enemy are flags.
#[test]
fn faction_crime_gold_is_paid_from_the_player_purse() {
    let mut g = game();
    let guards = obj(&mut g, "GuardsFaction");
    call(
        &mut g,
        &guards,
        "ModCrimeGold",
        &[Value::Int(40), Value::Bool(false)],
    );
    call(
        &mut g,
        &guards,
        "ModCrimeGold",
        &[Value::Int(100), Value::Bool(true)],
    );
    assert_eq!(call(&mut g, &guards, "GetCrimeGold", &[]), Value::Int(140));
    assert_eq!(
        call(&mut g, &guards, "GetCrimeGoldViolent", &[]),
        Value::Int(100)
    );
    assert_eq!(
        call(&mut g, &guards, "GetCrimeGoldNonViolent", &[]),
        Value::Int(40)
    );
    assert_eq!(call(&mut g, &guards, "GetInfamy", &[]), Value::Int(140));
    assert_eq!(
        call(&mut g, &guards, "GetInfamyViolent", &[]),
        Value::Int(100)
    );
    assert_eq!(
        call(&mut g, &guards, "CanPayCrimeGold", &[]),
        Value::Bool(false)
    );
    let player = g.player();
    let gold = g.form("Gold001").unwrap();
    g.world_mut().add_item(player, gold, 200);
    assert_eq!(
        call(&mut g, &guards, "CanPayCrimeGold", &[]),
        Value::Bool(true)
    );
    call(
        &mut g,
        &guards,
        "PlayerPayCrimeGold",
        &[Value::Bool(true), Value::Bool(true)],
    );
    assert_eq!(g.world_mut().item_count(player, gold), 60);
    assert_eq!(call(&mut g, &guards, "GetCrimeGold", &[]), Value::Int(0));
    assert_eq!(
        call(&mut g, &guards, "GetInfamyNonViolent", &[]),
        Value::Int(40)
    );
    call(&mut g, &guards, "SetCrimeGold", &[Value::Int(7)]);
    call(&mut g, &guards, "SetCrimeGoldViolent", &[Value::Int(3)]);
    assert_eq!(call(&mut g, &guards, "GetCrimeGold", &[]), Value::Int(10));
    assert_eq!(
        call(&mut g, &guards, "IsPlayerExpelled", &[]),
        Value::Bool(false)
    );
    call(&mut g, &guards, "SetPlayerExpelled", &[]);
    assert_eq!(
        call(&mut g, &guards, "IsPlayerExpelled", &[]),
        Value::Bool(true)
    );
    call(&mut g, &guards, "SetPlayerEnemy", &[Value::Bool(true)]);
    let id = g.form("GuardsFaction").unwrap();
    assert!(g.world_mut().faction(id).unwrap().player_enemy);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- actors and actor bases -----

/// AddPerk "adds the specified Perk to this actor", HasPerk and
/// RemovePerk, seeded from the base's PRKR list.
#[test]
fn perks_come_from_the_base_and_scripts_change_them() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let perk = obj(&mut g, "TestPerk");
    assert_eq!(
        call(&mut g, &guard, "HasPerk", &[perk.clone()]),
        Value::Bool(true)
    );
    call(&mut g, &guard, "RemovePerk", &[perk.clone()]);
    assert_eq!(
        call(&mut g, &guard, "HasPerk", &[perk.clone()]),
        Value::Bool(false)
    );
    call(&mut g, &guard, "AddPerk", &[perk.clone()]);
    assert_eq!(call(&mut g, &guard, "HasPerk", &[perk]), Value::Bool(true));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SetAlert and IsAlerted, StartSneaking toggles ("if the actor was
/// already sneaking, cancels sneaking"), DrawWeapon and SheatheWeapon
/// drive IsWeaponDrawn, SetRestrained and SetUnconscious exclude each
/// other, ClearArrested clears the flag.
#[test]
fn actor_flags_round_trip() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    assert_eq!(call(&mut g, &guard, "IsAlerted", &[]), Value::Bool(false));
    call(&mut g, &guard, "SetAlert", &[]);
    assert_eq!(call(&mut g, &guard, "IsAlerted", &[]), Value::Bool(true));
    call(&mut g, &guard, "SetAlert", &[Value::Bool(false)]);
    assert_eq!(call(&mut g, &guard, "IsAlerted", &[]), Value::Bool(false));
    call(&mut g, &guard, "StartSneaking", &[]);
    assert_eq!(call(&mut g, &guard, "IsSneaking", &[]), Value::Bool(true));
    call(&mut g, &guard, "StartSneaking", &[]);
    assert_eq!(call(&mut g, &guard, "IsSneaking", &[]), Value::Bool(false));
    call(&mut g, &guard, "DrawWeapon", &[]);
    assert_eq!(
        call(&mut g, &guard, "IsWeaponDrawn", &[]),
        Value::Bool(true)
    );
    call(&mut g, &guard, "SheatheWeapon", &[]);
    assert_eq!(
        call(&mut g, &guard, "IsWeaponDrawn", &[]),
        Value::Bool(false)
    );
    call(&mut g, &guard, "SetUnconscious", &[]);
    call(&mut g, &guard, "SetRestrained", &[]);
    assert_eq!(
        call(&mut g, &guard, "IsUnconscious", &[]),
        Value::Bool(false)
    );
    let id = g.form("GuardRef").unwrap();
    assert!(g.world_mut().actor(id).unwrap().restrained);
    call(&mut g, &guard, "SetUnconscious", &[]);
    assert!(!g.world_mut().actor(id).unwrap().restrained);
    g.world_mut().actor(id).unwrap().arrested = true;
    assert_eq!(call(&mut g, &guard, "IsArrested", &[]), Value::Bool(true));
    call(&mut g, &guard, "ClearArrested", &[]);
    assert_eq!(call(&mut g, &guard, "IsArrested", &[]), Value::Bool(false));
    call(&mut g, &guard, "SetAttackActorOnSight", &[]);
    call(&mut g, &guard, "SetDontMove", &[]);
    call(&mut g, &guard, "SetAllowPCDialogue", &[Value::Bool(false)]);
    let a = g.world_mut().actor(id).unwrap().clone();
    assert!(a.attack_on_sight && a.dont_move && !a.allow_pc_dialogue);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SetRelationshipRank "sets the relationship rank between this actor
/// and another" (4 lover to -4 archnemesis), GetRelationshipRank reads
/// it from either side, the highest and lowest scan every other actor.
#[test]
fn relationship_ranks_are_symmetric() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let player = obj(&mut g, "PlayerRef");
    assert_eq!(
        call(&mut g, &guard, "GetRelationshipRank", &[player.clone()]),
        Value::Int(0)
    );
    call(
        &mut g,
        &guard,
        "SetRelationshipRank",
        &[player.clone(), Value::Int(-4)],
    );
    assert_eq!(
        call(&mut g, &guard, "GetRelationshipRank", &[player.clone()]),
        Value::Int(-4)
    );
    assert_eq!(
        call(&mut g, &player, "GetRelationshipRank", &[guard.clone()]),
        Value::Int(-4)
    );
    call(
        &mut g,
        &player,
        "SetRelationshipRank",
        &[guard.clone(), Value::Int(9)],
    );
    assert_eq!(
        call(&mut g, &guard, "GetRelationshipRank", &[player]),
        Value::Int(4)
    );
    assert_eq!(
        call(&mut g, &guard, "GetHighestRelationshipRank", &[]),
        Value::Int(4)
    );
    assert_eq!(
        call(&mut g, &guard, "GetLowestRelationshipRank", &[]),
        Value::Int(4)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// StartCombat records the target GetCombatTarget returns, StopCombat
/// clears it, Kill(akKiller) records who GetKiller answers, and
/// GetKiller "if this actor isn't dead, returns None".
#[test]
fn combat_targets_and_killers_are_remembered() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let player = obj(&mut g, "PlayerRef");
    assert_eq!(call(&mut g, &guard, "GetCombatTarget", &[]), Value::None);
    call(&mut g, &guard, "StartCombat", &[player.clone()]);
    let target = call(&mut g, &guard, "GetCombatTarget", &[]);
    assert!(is(&mut g, &target, "PlayerRef"));
    call(&mut g, &guard, "StopCombat", &[]);
    assert_eq!(call(&mut g, &guard, "GetCombatTarget", &[]), Value::None);
    assert_eq!(call(&mut g, &player, "GetKiller", &[]), Value::None);
    call(&mut g, &player, "Kill", &[guard.clone()]);
    let killer = call(&mut g, &player, "GetKiller", &[]);
    assert!(is(&mut g, &killer, "GuardRef"));
    call(&mut g, &guard, "Kill", &[player]);
    assert_eq!(
        call(&mut g, &guard, "GetKiller", &[]),
        Value::None,
        "essential actors do not die"
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// GetEquippedWeapon selects the right hand by default and the left hand
/// when true. GetEquippedShield returns equipped armor in BOD2 shield slot 9.
#[test]
fn scr_14_equipped_weapon_hands_and_shield_follow_equipment() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let actor = g.form("GuardRef").unwrap();
    let sword = g.form("IronSword").unwrap();
    let shield = GlobalFormId {
        file: 0,
        object: SHIELD,
    };
    let dagger = GlobalFormId {
        file: 0,
        object: DAGGER,
    };
    assert!(g.world_mut().record(shield).is_some());
    assert_eq!(
        g.world_mut().record(shield).unwrap().find(T(b"BOD2")),
        Some(
            [le(1 << 9).as_slice(), le(2).as_slice()]
                .concat()
                .as_slice()
        )
    );
    assert!(g.world_mut().record(dagger).is_some());

    assert_eq!(call(&mut g, &guard, "GetEquippedWeapon", &[]), Value::None);
    assert_eq!(
        call(&mut g, &guard, "GetEquippedWeapon", &[Value::Bool(true)]),
        Value::None
    );
    assert_eq!(call(&mut g, &guard, "GetEquippedShield", &[]), Value::None);

    g.world_mut().equip_item(actor, sword).unwrap();
    g.world_mut().equip_item(actor, shield).unwrap();
    g.world_mut().equip_item(actor, dagger).unwrap();
    let mut world = g.world_mut();
    let state = world.actor(actor).unwrap();
    state.left_weapon = Some(dagger);
    state.right_weapon = Some(sword);
    drop(world);
    assert!(g
        .world_mut()
        .actor(actor)
        .unwrap()
        .equipped
        .contains(&shield));
    assert_eq!(g.world_mut().item_count(actor, shield), 1);
    assert_eq!(g.world_mut().equipped_shield(actor).unwrap(), Some(shield));

    let right = call(&mut g, &guard, "GetEquippedWeapon", &[]);
    let left = call(&mut g, &guard, "GetEquippedWeapon", &[Value::Bool(true)]);
    assert!(is(&mut g, &right, "IronSword"));
    assert!(matches!(
        left,
        Value::Object(object) if g.world_mut().form_of(object) == Some(dagger)
    ));
    let equipped_shield = call(&mut g, &guard, "GetEquippedShield", &[]);
    assert!(matches!(
        equipped_shield,
        Value::Object(object) if g.world_mut().form_of(object) == Some(shield)
    ));

    g.world_mut().unequip_item(actor, shield).unwrap();
    assert_eq!(call(&mut g, &guard, "GetEquippedShield", &[]), Value::None);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// GetEquippedShield returns no form and logs its typed record error when an
/// equipped armor record has no required physical fields.
#[test]
fn scr_14_equipped_shield_reports_malformed_equipment() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let actor = g.form("GuardRef").unwrap();
    let broken = GlobalFormId {
        file: 0,
        object: BROKEN_ARMOR,
    };
    g.world_mut().add_item(actor, broken, 1);
    g.world_mut().actor(actor).unwrap().equipped.insert(broken);

    assert_eq!(call(&mut g, &guard, "GetEquippedShield", &[]), Value::None);
    assert!(
        g.traces()
            .iter()
            .any(|trace| trace.contains("Actor.GetEquippedShield:")),
        "{:?}",
        g.traces()
    );
}

/// GetCrimeFaction reads CRIF, SetCrimeFaction "if the actor does not
/// currently belong to the crime faction, it will be added to that
/// faction", SetOutfit changes the default or the sleep outfit.
#[test]
fn crime_faction_and_outfit_follow_the_pages() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let crime = call(&mut g, &guard, "GetCrimeFaction", &[]);
    assert!(is(&mut g, &crime, "GuardsFaction"));
    let thieves = obj(&mut g, "ThievesFaction");
    assert_eq!(
        call(&mut g, &guard, "IsInFaction", &[thieves.clone()]),
        Value::Bool(false)
    );
    call(&mut g, &guard, "SetCrimeFaction", &[thieves.clone()]);
    assert_eq!(
        call(&mut g, &guard, "IsInFaction", &[thieves]),
        Value::Bool(true)
    );
    call(&mut g, &guard, "SetCrimeFaction", &[Value::None]);
    assert_eq!(call(&mut g, &guard, "GetCrimeFaction", &[]), Value::None);
    let rags = obj(&mut g, "Rags");
    call(
        &mut g,
        &guard,
        "SetOutfit",
        &[rags.clone(), Value::Bool(true)],
    );
    let id = g.form("GuardRef").unwrap();
    let rags_id = g.form("Rags").unwrap();
    let outfit_id = g.form("GuardOutfit").unwrap();
    let a = g.world_mut().actor(id).unwrap().clone();
    assert_eq!(a.sleep_outfit, Some(rags_id));
    assert_eq!(a.outfit, Some(outfit_id));
    call(&mut g, &guard, "SetOutfit", &[rags, Value::Bool(false)]);
    assert_eq!(g.world_mut().actor(id).unwrap().outfit, Some(rags_id));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// ActorBase reads its ACBS flags (IsEssential, IsUnique, GetSex 1 for
/// female), the setters flip them, GetRace and GetClass read RNAM and
/// CNAM, GetDeadCount starts at zero.
#[test]
fn actor_base_flags_and_setters() {
    let mut g = game();
    let base = obj(&mut g, "GuardBase");
    assert_eq!(call(&mut g, &base, "IsEssential", &[]), Value::Bool(true));
    assert_eq!(call(&mut g, &base, "IsUnique", &[]), Value::Bool(true));
    assert_eq!(call(&mut g, &base, "IsProtected", &[]), Value::Bool(false));
    assert_eq!(
        call(&mut g, &base, "IsInvulnerable", &[]),
        Value::Bool(false)
    );
    assert_eq!(call(&mut g, &base, "GetSex", &[]), Value::Int(1));
    assert_eq!(call(&mut g, &base, "GetDeadCount", &[]), Value::Int(0));
    assert_eq!(call(&mut g, &base, "GetRace", &[]), Value::None);
    assert_eq!(call(&mut g, &base, "GetClass", &[]), Value::None);
    call(&mut g, &base, "SetEssential", &[Value::Bool(false)]);
    call(&mut g, &base, "SetProtected", &[Value::Bool(true)]);
    call(&mut g, &base, "SetInvulnerable", &[Value::Bool(true)]);
    assert_eq!(call(&mut g, &base, "IsEssential", &[]), Value::Bool(false));
    assert_eq!(call(&mut g, &base, "IsProtected", &[]), Value::Bool(true));
    assert_eq!(
        call(&mut g, &base, "IsInvulnerable", &[]),
        Value::Bool(true)
    );
    let rags = obj(&mut g, "Rags");
    call(&mut g, &base, "SetOutfit", &[rags, Value::Bool(false)]);
    let id = g.form("GuardBase").unwrap();
    let rags_id = g.form("Rags").unwrap();
    assert_eq!(g.world_mut().actor_base(id).unwrap().outfit, Some(rags_id));
    let player_base = obj(&mut g, "PlayerBase");
    assert_eq!(call(&mut g, &player_base, "GetSex", &[]), Value::Int(0));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// SCR-11: Actor.SetEssential updates its base-data source and every loaded
/// actor inheriting that source through the shared world API.
#[test]
fn scr_11_actor_set_essential_updates_shared_actor_state() {
    let mut g = game();
    let actor = obj(&mut g, "GuardRef");
    let base = g.form("GuardBase").unwrap();
    let actor_id = g.form("GuardRef").unwrap();

    assert_eq!(call(&mut g, &actor, "IsEssential", &[]), Value::Bool(true));
    assert_eq!(
        call(&mut g, &actor, "SetEssential", &[Value::Bool(false)]),
        Value::None
    );
    assert!(!g.world_mut().actor_base(base).unwrap().essential);
    assert!(!g.world_mut().actor(actor_id).unwrap().essential);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));

    call(&mut g, &actor, "SetEssential", &[Value::Bool(true)]);
    assert!(g.world_mut().actor_base(base).unwrap().essential);
    assert!(g.world_mut().actor(actor_id).unwrap().essential);
}

/// SCR-12: expression setters validate CK ranges and restore record mood on
/// clear. Head tracking uses the CK default, accepts its optional argument,
/// and persists an explicit value through the actor section.
#[test]
fn scr_12_expression_and_head_tracking_actor_natives() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let id = g.form("GuardRef").unwrap();
    assert_eq!(g.world_mut().head_tracking(id), Some(true));

    call(&mut g, &guard, "SetHeadTracking", &[Value::Bool(false)]);
    assert_eq!(g.world_mut().head_tracking(id), Some(false));
    call(&mut g, &guard, "SetHeadTracking", &[]);
    assert_eq!(g.world_mut().head_tracking(id), Some(true));
    call(&mut g, &guard, "SetHeadTracking", &[Value::Bool(false)]);

    call(
        &mut g,
        &guard,
        "SetExpressionOverride",
        &[Value::Int(12), Value::Int(45)],
    );
    assert_eq!(g.world_mut().facial_expression(id), Some((12, 45)));
    call(&mut g, &guard, "ClearExpressionOverride", &[]);
    let cleared = g.world_mut().facial_expression(id);
    assert_ne!(cleared, Some((12, 45)));

    call(
        &mut g,
        &guard,
        "SetExpressionOverride",
        &[Value::Int(17), Value::Int(45)],
    );
    assert!(g
        .world_mut()
        .set_expression_override(id, 17, 45)
        .unwrap_err()
        .contains("expression mood 17"));
    assert_eq!(g.world_mut().facial_expression(id), cleared);

    let saved = g.world_mut().save_state();
    let loaded = game();
    loaded.world_mut().load_state(&saved).unwrap();
    let loaded_id = loaded.form("GuardRef").unwrap();
    assert_eq!(loaded.world_mut().head_tracking(loaded_id), Some(false));
}

#[test]
fn anm_59_expression_override_holds_until_clear_then_restores_record_mood() {
    let mut g = game();
    let actor = obj(&mut g, "GuardRef");
    let id = g.form("GuardRef").unwrap();
    let record_mood = g.world_mut().facial_expression(id);

    call(
        &mut g,
        &actor,
        "SetExpressionOverride",
        &[Value::Int(12), Value::Int(45)],
    );
    assert_eq!(g.world_mut().facial_expression(id), Some((12, 45)));
    assert_eq!(g.world_mut().facial_expression(id), Some((12, 45)));

    call(&mut g, &actor, "ClearExpressionOverride", &[]);
    assert_eq!(g.world_mut().facial_expression(id), record_mood);
}

// ----- references, cells, ownership -----

/// GetActorOwner "gets the ActorBase that owns this object", None when
/// the owner is a faction and the other way round, SetActorOwner(None)
/// clears ownership. The same four on a Cell read the cell's XOWN.
#[test]
fn ownership_is_typed_by_the_owner_record() {
    let mut g = game();
    let marker = obj(&mut g, "MarkerRef");
    let owner = call(&mut g, &marker, "GetActorOwner", &[]);
    assert!(is(&mut g, &owner, "PlayerBase"));
    assert_eq!(call(&mut g, &marker, "GetFactionOwner", &[]), Value::None);
    let guards = obj(&mut g, "GuardsFaction");
    call(&mut g, &marker, "SetFactionOwner", &[guards]);
    assert_eq!(call(&mut g, &marker, "GetActorOwner", &[]), Value::None);
    let owner = call(&mut g, &marker, "GetFactionOwner", &[]);
    assert!(is(&mut g, &owner, "GuardsFaction"));
    call(&mut g, &marker, "SetActorOwner", &[Value::None]);
    assert_eq!(call(&mut g, &marker, "GetFactionOwner", &[]), Value::None);
    let cell = obj(&mut g, "InnCell");
    let owner = call(&mut g, &cell, "GetFactionOwner", &[]);
    assert!(is(&mut g, &owner, "GuardsFaction"));
    assert_eq!(call(&mut g, &cell, "GetActorOwner", &[]), Value::None);
    let base = obj(&mut g, "GuardBase");
    call(&mut g, &cell, "SetActorOwner", &[base]);
    let owner = call(&mut g, &cell, "GetActorOwner", &[]);
    assert!(is(&mut g, &owner, "GuardBase"));
    call(&mut g, &cell, "SetPublic", &[]);
    let id = g.form("InnCell").unwrap();
    assert!(g.world_mut().cell_state(id).unwrap().public);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Cell.IsAttached reports current streamed residency. Cell.Reset marks the
/// cell for a record-backed temporary-reference rebuild at its next load.
#[test]
fn scr_19_cell_is_attached_tracks_cell_residency() {
    let mut g = game();
    let cell = obj(&mut g, "InnCell");
    let id = g.form("InnCell").unwrap();
    g.world_mut().unload_cell(id);
    assert_eq!(call(&mut g, &cell, "IsAttached", &[]), Value::Bool(false));
    g.world_mut().prefetch_cell(id);
    assert_eq!(call(&mut g, &cell, "IsAttached", &[]), Value::Bool(false));
    g.world_mut().load_cell(id);
    assert_eq!(call(&mut g, &cell, "IsAttached", &[]), Value::Bool(true));
    g.world_mut().unload_cell(id);
    assert_eq!(call(&mut g, &cell, "IsAttached", &[]), Value::Bool(false));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn scr_19_cell_reset_restores_temporary_references_on_next_load() {
    let mut g = game();
    let cell = obj(&mut g, "InnCell");
    let marker = obj(&mut g, "MarkerRef");
    let cell_id = g.form("InnCell").unwrap();
    let marker_id = g.form("MarkerRef").unwrap();
    g.world_mut().load_cell(cell_id);
    call(&mut g, &marker, "Disable", &[]);
    assert!(!g.world_mut().reference(marker_id).unwrap().enabled);
    assert_eq!(call(&mut g, &cell, "Reset", &[]), Value::None);
    let saved = g.world_mut().save_state();
    g.world_mut().load_state(&saved).unwrap();
    assert_eq!(call(&mut g, &cell, "IsAttached", &[]), Value::Bool(false));
    g.world_mut().unload_cell(cell_id);
    g.world_mut().load_cell(cell_id);
    assert!(g.world_mut().reference(marker_id).unwrap().enabled);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn scr_19_cell_reset_rejects_non_cell_forms() {
    let g = game();
    let marker_id = g.form("MarkerRef").unwrap();
    let err = g.world_mut().reset_cell(marker_id).unwrap_err();
    assert!(err.to_string().contains("invalid cell reset target"));
}

/// AddToMap "adds this reference (which must be a map marker) to the
/// player's map, optionally making it available for fast-travel",
/// EnableFastTravel flips that, SetDestroyed sets the destroyed flag,
/// IgnoreFriendlyHits drives IsIgnoringFriendlyHits.
#[test]
fn map_markers_and_reference_flags() {
    let mut g = game();
    let marker = obj(&mut g, "MarkerRef");
    let id = g.form("MarkerRef").unwrap();
    call(&mut g, &marker, "AddToMap", &[Value::Bool(false)]);
    let r = g.world_mut().reference(id).unwrap().clone();
    assert!(r.on_map && !r.fast_travel);
    call(&mut g, &marker, "EnableFastTravel", &[]);
    assert!(g.world_mut().reference(id).unwrap().fast_travel);
    call(&mut g, &marker, "SetDestroyed", &[]);
    assert!(g.world_mut().reference(id).unwrap().destroyed);
    call(&mut g, &marker, "SetDestroyed", &[Value::Bool(false)]);
    assert!(!g.world_mut().reference(id).unwrap().destroyed);
    assert_eq!(
        call(&mut g, &marker, "IsIgnoringFriendlyHits", &[]),
        Value::Bool(false)
    );
    call(&mut g, &marker, "IgnoreFriendlyHits", &[]);
    assert_eq!(
        call(&mut g, &marker, "IsIgnoringFriendlyHits", &[]),
        Value::Bool(true)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// AddInventoryEventFilter filters "stack with any previous filters",
/// RemoveInventoryEventFilter drops one, RemoveAllInventoryEventFilters
/// the lot, on a reference and on a reference alias alike.
#[test]
fn inventory_event_filters_stack() {
    let mut g = game();
    g.start();
    let marker = obj(&mut g, "MarkerRef");
    let gold = obj(&mut g, "Gold001");
    let sword = obj(&mut g, "IronSword");
    call(&mut g, &marker, "AddInventoryEventFilter", &[gold.clone()]);
    call(&mut g, &marker, "AddInventoryEventFilter", &[sword.clone()]);
    let object = marker.as_object().unwrap();
    assert_eq!(g.world().inventory_filters[&object].len(), 2);
    call(
        &mut g,
        &marker,
        "RemoveInventoryEventFilter",
        &[gold.clone()],
    );
    assert_eq!(g.world().inventory_filters[&object].len(), 1);
    call(&mut g, &marker, "RemoveAllInventoryEventFilters", &[]);
    assert!(!g.world().inventory_filters.contains_key(&object));
    let quest = g.form("LocQuest").unwrap();
    let alias = Value::Object(g.object_of(Handle::Alias { quest, id: 1 }));
    call(&mut g, &alias, "AddInventoryEventFilter", &[gold]);
    let object = alias.as_object().unwrap();
    assert_eq!(g.world().inventory_filters[&object].len(), 1);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// CK lists Alias.GetOwningQuest and the five GlobalVariable members. The
/// shipped GlobalVariable script defines the integer methods through casts.
#[test]
fn scr_62_alias_owner_and_global_variable_methods_follow_script_contracts() {
    let mut g = game();
    let quest = g.form("LocQuest").unwrap();
    let alias = Value::Object(g.object_of(Handle::Alias { quest, id: 1 }));
    assert_eq!(
        call(&mut g, &alias, "GetOwningQuest", &[]),
        obj(&mut g, "LocQuest")
    );

    let global = obj(&mut g, "Counter");
    assert_eq!(call(&mut g, &global, "GetValue", &[]), Value::Float(0.0));
    assert_eq!(call(&mut g, &global, "GetValueInt", &[]), Value::Int(0));
    call(&mut g, &global, "SetValue", &[Value::Float(7.75)]);
    assert_eq!(call(&mut g, &global, "GetValue", &[]), Value::Float(7.75));
    assert_eq!(call(&mut g, &global, "GetValueInt", &[]), Value::Int(7));
    call(&mut g, &global, "SetValueInt", &[Value::Int(4)]);
    assert_eq!(call(&mut g, &global, "GetValue", &[]), Value::Float(4.0));
    assert_eq!(
        call(&mut g, &global, "Mod", &[Value::Float(1.5)]),
        Value::Float(5.5)
    );
    assert_eq!(call(&mut g, &global, "GetValue", &[]), Value::Float(5.5));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// CK's AddItem page says that a FormList adds aiCount of every member. The
/// queued events are one OnItemAdded per base form, carrying that same count.
#[test]
fn add_item_expands_form_list_with_member_counts_and_events() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let list = obj(&mut g, "AddList");
    let guard_id = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    let add_list = g.form("AddList").unwrap();
    call(&mut g, &guard, "AddItem", &[list, Value::Int(3)]);
    assert_eq!(g.world_mut().item_count(guard_id, gold), 3);
    assert_eq!(g.world_mut().item_count(guard_id, sword), 3);

    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(world.add_form_list_notified(guard_id, add_list, 2, &mut alloc));
    let events: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnItemAdded" => {
                Some((args[0].clone(), args[1].clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].1, Value::Int(2));
    assert_eq!(events[1].1, Value::Int(2));
}

/// A FormList resolves every LVLI occurrence with its receiver's AddItem
/// context, then combines the selected result with its ordinary members.
#[test]
fn add_item_expands_mixed_form_list_with_leveled_metadata() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let list = obj(&mut g, "NestedLeveledAddList");
    let loot = g.form("LootList").unwrap();
    g.world_mut().leveled_list(loot).unwrap().chance_none = 0;
    let guard_id = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    call(&mut g, &guard, "AddItem", &[list, Value::Int(2)]);
    assert_eq!(g.world_mut().item_count(guard_id, gold), 12);
    assert_eq!(g.world_mut().item_count(guard_id, sword), 2);
    let world = g.world();
    let stack = &world
        .reference_ref(guard_id)
        .unwrap()
        .inventory
        .stacks(sword)[0];
    assert_eq!(stack.count(), 2);
    assert_eq!(
        stack.extra(37).unwrap().fields,
        vec![soe_game::inventory::Field::FloatBits(1.25_f32.to_bits())]
    );
}

/// A later reference capacity failure rolls the earlier LVLI during preflight,
/// then restores its RNG state and leaves all ordinary, selected and reference
/// additions untouched.
#[test]
fn add_item_preflights_mixed_base_leveled_and_reference_atomically() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    let list = g.form("MixedReferenceAddList").unwrap();
    let loose = g.form("LooseSwordRef").unwrap();
    let loot_list = g.form("LootList").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    world.add_item(guard, sword, i32::MAX - 1);
    world.leveled_list(loot_list).unwrap();
    let saved = world.save_state();
    let expected_rng = world.random_u64_for(RngStream::LeveledItems);
    world.load_state(&saved).unwrap();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(!world.add_form_list_notified(guard, list, 1, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 0);
    assert_eq!(world.item_count(guard, sword), i32::MAX - 1);
    assert!(world.reference_ref(loose).unwrap().item.is_some());
    assert!(world.pending.is_empty());
    assert_eq!(world.random_u64_for(RngStream::LeveledItems), expected_rng);
}

/// A later overflowing member rejects the complete list before the preceding
/// member changes or queues an event.
#[test]
fn add_item_rejects_later_member_overflow_without_partial_addition() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    let list = g.form("AddList").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    world.add_item(guard, sword, i32::MAX);
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(!world.add_form_list_notified(guard, list, 1, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 0);
    assert_eq!(world.item_count(guard, sword), i32::MAX);
    assert!(world.pending.is_empty());
}

/// A reference member repeats neither its item identity nor its loose stack.
/// Its count must be one, matching INV-01's direct AddItem contract.
#[test]
fn add_item_rejects_reference_member_without_partial_addition() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let list = obj(&mut g, "ReferenceAddList");
    let guard_id = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    call(&mut g, &guard, "AddItem", &[list, Value::Int(2)]);
    assert_eq!(g.world_mut().item_count(guard_id, gold), 0);
}

/// FormList reference members retain their original-reference identity and
/// loose-item metadata through INV-01's transfer path.
#[test]
fn add_item_transfers_form_list_reference_with_metadata_and_events() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    let list = g.form("ReferenceAddList").unwrap();
    let loose = g.form("LooseSwordRef").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(world.add_form_list_notified(guard, list, 1, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 1);
    assert_eq!(world.item_count(guard, sword), 2);
    let stack = &world.reference_ref(guard).unwrap().inventory.stacks(sword)[0];
    assert_eq!(stack.reference(), Some(loose));
    assert!(stack.extra(40).is_some());
    let state = world.reference_ref(loose).unwrap();
    assert!(state.item.is_none());
    assert!(!state.enabled);
    let names: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, ["OnItemAdded", "OnItemAdded", "OnContainerChanged"]);
}

/// A nested or malformed list cannot move one concrete reference twice. The
/// complete call fails before its earlier base member changes or queues work.
#[test]
fn add_item_rejects_repeated_reference_without_partial_addition() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let loose = g.form("LooseSwordRef").unwrap();
    let list = g.form("RepeatedReferenceAddList").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(!world.add_form_list_notified(guard, list, 1, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 0);
    assert!(world.reference_ref(loose).unwrap().item.is_some());
    assert!(world.pending.is_empty());
}

/// The reference contribution joins base forms in the same checked capacity
/// preflight. Overflow leaves the loose reference, the earlier base and all
/// event queues unchanged.
#[test]
fn add_item_preflights_reference_overflow_before_base_mutation() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    let loose = g.form("LooseSwordRef").unwrap();
    let list = g.form("ReferenceAddList").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    world.add_item(guard, sword, i32::MAX - 1);
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(!world.add_form_list_notified(guard, list, 1, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 0);
    assert_eq!(world.item_count(guard, sword), i32::MAX - 1);
    assert!(world.reference_ref(loose).unwrap().item.is_some());
    assert!(world.pending.is_empty());
}

/// Nested FormLists flatten in depth-first member order. Each occurrence
/// applies aiCount, so a shared child list contributes each time it appears.
#[test]
fn add_item_flattens_nested_and_shared_form_lists_in_member_order() {
    let mut g = game();
    let guard = obj(&mut g, "GuardRef");
    let list = obj(&mut g, "NestedAddList");
    let guard_id = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let sword = g.form("IronSword").unwrap();
    let gold_object = g.object_of_form(gold);
    let sword_object = g.object_of_form(sword);
    call(&mut g, &guard, "AddItem", &[list, Value::Int(2)]);
    assert_eq!(g.world_mut().item_count(guard_id, gold), 2);
    assert_eq!(g.world_mut().item_count(guard_id, sword), 2);

    let shared = g.form("SharedAddList").unwrap();
    let shared_world = g.shared_world();
    let mut world = shared_world.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(world.add_form_list_notified(guard_id, shared, 2, &mut alloc));
    assert_eq!(world.item_count(guard_id, gold), 6);
    assert_eq!(world.item_count(guard_id, sword), 6);
    let events: Vec<_> = world
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnItemAdded" => Some(args),
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0][0], Value::Object(gold_object));
    assert_eq!(events[1][0], Value::Object(sword_object));
    assert_eq!(events[0][1], Value::Int(4));
    assert_eq!(events[1][1], Value::Int(4));
}

/// Cyclic or over-deep lists reject during preflight, so they neither add the
/// reachable base item nor queue an inventory event.
#[test]
fn add_item_rejects_cyclic_and_overdeep_form_lists_atomically() {
    for name in ["CycleAddListA", "DeepAddList0"] {
        let mut g = game();
        let guard = g.form("GuardRef").unwrap();
        let gold = g.form("Gold001").unwrap();
        let list = g.form(name).unwrap();
        let shared = g.shared_world();
        let mut world = shared.borrow_mut();
        let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
        assert!(!world.add_form_list_notified(guard, list, 1, &mut alloc));
        assert_eq!(world.item_count(guard, gold), 0);
        assert!(world.pending.is_empty());
    }
}

/// CK says FormLists cannot contain duplicates. If malformed record data or
/// the current script mutation path supplies one, aggregate it by base form.
/// OnItemAdded documents one queued call per base form for bulk additions.
#[test]
fn add_item_aggregates_duplicate_members_into_one_event() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let list = g.form("DuplicateAddList").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(world.add_form_list_notified(guard, list, 2, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 4);
    let events: Vec<_> = world
        .pending
        .iter()
        .filter(|pending| matches!(pending, Pending::Event { name, .. } if name == "OnItemAdded"))
        .collect();
    assert_eq!(events.len(), 1);
    let Pending::Event { args, .. } = events[0] else {
        unreachable!();
    };
    assert_eq!(args[1], Value::Int(4));
}

/// Empty lists are successful no-ops. The direct helper rejects zero and
/// negative counts so non-script callers cannot manufacture a count change.
#[test]
fn add_item_form_list_empty_and_nonpositive_counts_do_not_mutate() {
    let mut g = game();
    let guard = g.form("GuardRef").unwrap();
    let gold = g.form("Gold001").unwrap();
    let list = g.form("EmptyAddList").unwrap();
    let shared = g.shared_world();
    let mut world = shared.borrow_mut();
    let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
    assert!(world.add_form_list_notified(guard, list, 1, &mut alloc));
    assert!(!world.add_form_list_notified(guard, list, 0, &mut alloc));
    assert!(!world.add_form_list_notified(guard, list, -1, &mut alloc));
    assert_eq!(world.item_count(guard, gold), 0);
    assert!(world.pending.is_empty());
}

// ----- listeners on Form -----

#[test]
fn anm_58_graph_events_resume_named_waits_true() {
    let mut g = game();
    let sender = obj(&mut g, "GuardRef");
    let source = g.form("GuardRef").unwrap();
    let thread = wait_for_animation_thread(&mut g, &sender, "done");
    assert_eq!(g.vm.thread_result(thread), None);

    g.report_animation_event(source, "other");
    g.pump();
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(thread), None);

    g.report_animation_event(source, "DONE");
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(thread), Some(Value::Bool(true)));
    assert_eq!(
        g.world().animation_events,
        vec![(source, "other".into()), (source, "DONE".into())]
    );
}

/// `WaitForAnimationEvent` takes the event name from its sole argument and
/// waits on the called reference, matching names without case sensitivity.
#[test]
fn scr_41_wait_for_animation_event_uses_reference_and_event_argument() {
    let mut g = game();
    let sender = obj(&mut g, "GuardRef");
    let source = g.form("GuardRef").unwrap();
    let thread = wait_for_animation_thread(&mut g, &sender, "finished");

    let unrelated = g.form("MarkerRef").unwrap();
    g.report_animation_event(unrelated, "finished");
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(thread), None);

    g.report_animation_event(source, "other");
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(thread), None);

    g.report_animation_event(source, "FINISHED");
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(thread), Some(Value::Bool(true)));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// A disabled reference and an unloaded animation graph finish the wait
/// with false.
#[test]
fn scr_41_wait_for_animation_event_reports_false_when_unavailable() {
    let mut g = game();
    let sender = obj(&mut g, "GuardRef");
    let source = g.form("GuardRef").unwrap();
    let unloaded = wait_for_animation_thread(&mut g, &sender, "finished");
    g.animation_graph_unloaded(source);
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(unloaded), Some(Value::Bool(false)));

    let disabled = wait_for_animation_thread(&mut g, &sender, "finished");
    call(&mut g, &sender, "DisableNoWait", &[]);
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(disabled), Some(Value::Bool(false)));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn anm_58_graph_unload_and_reference_disable_resume_false() {
    let mut g = game();
    let sender = obj(&mut g, "GuardRef");
    let source = g.form("GuardRef").unwrap();
    let unloaded = wait_for_animation_thread(&mut g, &sender, "finished");
    g.animation_graph_unloaded(source);
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(unloaded), Some(Value::Bool(false)));

    let disabled = wait_for_animation_thread(&mut g, &sender, "finished");
    call(&mut g, &sender, "DisableNoWait", &[]);
    g.run_until_idle(64);
    assert_eq!(g.vm.thread_result(disabled), Some(Value::Bool(false)));
}

#[test]
fn anm_58_wait_registration_survives_world_save_load() {
    let mut g = game();
    let sender = obj(&mut g, "GuardRef");
    let source = g.form("GuardRef").unwrap();
    let _thread = wait_for_animation_thread(&mut g, &sender, "finished");
    let saved = g.world().save_state();
    g.world_mut().load_state(&saved).unwrap();
    g.world_mut().animation_graph_unloaded(source);
    assert_eq!(g.world().completed_animation_waits.len(), 1);
    assert!(!g.world().completed_animation_waits[0].1);
}

/// RegisterForAnimationEvent "registers this form to receive the specified
/// animation event from the specified object". Host graph reports reach
/// OnAnimationEvent(akSource, asEventName) on the listener, and
/// UnregisterForAnimationEvent stops delivery.
#[test]
fn animation_events_reach_registered_listeners() {
    let mut g = game();
    g.start();
    let marker = obj(&mut g, "MarkerRef");
    let guard = obj(&mut g, "GuardRef");
    let source = g.form("GuardRef").unwrap();
    call(&mut g, &guard, "PlayAnimation", &[Value::string("IdleHit")]);
    g.report_animation_event(source, "IdleHit");
    assert!(!g.traces().iter().any(|t| t == "IdleHit"));
    assert_eq!(
        call(
            &mut g,
            &marker,
            "RegisterForAnimationEvent",
            &[guard.clone(), Value::string("IdleHit")]
        ),
        Value::Bool(true)
    );
    call(&mut g, &guard, "PlayAnimation", &[Value::string("IdleHit")]);
    assert!(!g.traces().iter().any(|t| t == "IdleHit"));
    g.report_animation_event(source, "IdleHit");
    assert!(
        g.traces().iter().any(|t| t == "IdleHit"),
        "{:?}",
        g.traces()
    );
    call(
        &mut g,
        &guard,
        "PlayAnimation",
        &[Value::string("IdleOther")],
    );
    g.report_animation_event(source, "IdleOther");
    assert!(!g.traces().iter().any(|t| t == "IdleOther"));
    call(
        &mut g,
        &marker,
        "UnregisterForAnimationEvent",
        &[guard.clone(), Value::string("IdleHit")],
    );
    call(&mut g, &guard, "PlayAnimation", &[Value::string("IdleHit")]);
    g.report_animation_event(source, "IdleHit");
    assert_eq!(g.traces().iter().filter(|t| *t == "IdleHit").count(), 1);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// RegisterForTrackedStatsEvent makes Game.IncrementStat deliver
/// OnTrackedStatsEvent(asStat, aiStatValue) with the updated value,
/// QueryStat reads it, UnregisterForTrackedStatsEvent stops the events.
#[test]
fn tracked_stats_fire_events() {
    let mut g = game();
    g.start();
    let marker = obj(&mut g, "MarkerRef");
    call_global(
        &mut g,
        "Game",
        "IncrementStat",
        &[Value::string("Barters"), Value::Int(1)],
    );
    assert_eq!(
        call_global(&mut g, "Game", "QueryStat", &[Value::string("Barters")]),
        Value::Int(1)
    );
    assert!(!g.traces().iter().any(|t| t == "Barters"));
    call(&mut g, &marker, "RegisterForTrackedStatsEvent", &[]);
    call_global(
        &mut g,
        "Game",
        "IncrementStat",
        &[Value::string("Barters"), Value::Int(2)],
    );
    assert_eq!(
        call_global(&mut g, "Game", "QueryStat", &[Value::string("Barters")]),
        Value::Int(3)
    );
    assert!(
        g.traces().iter().any(|t| t == "Barters"),
        "{:?}",
        g.traces()
    );
    call(&mut g, &marker, "UnregisterForTrackedStatsEvent", &[]);
    call_global(
        &mut g,
        "Game",
        "IncrementStat",
        &[Value::string("Barters"), Value::Int(1)],
    );
    assert_eq!(g.traces().iter().filter(|t| *t == "Barters").count(), 1);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn ui_17_chargen_requests_precache_before_menu_and_clear_afterward() {
    let mut g = game();
    assert_eq!(
        call_global(&mut g, "Game", "PrecacheCharGen", &[]),
        Value::None
    );
    assert_eq!(
        call_global(&mut g, "Game", "ShowRaceMenu", &[]),
        Value::None
    );
    assert_eq!(
        call_global(&mut g, "Game", "PrecacheCharGenClear", &[]),
        Value::None
    );
    assert_eq!(
        g.world_mut().menu_requests,
        [
            soe_game::world::MenuRequest::PrecacheCharGen,
            soe_game::world::MenuRequest::Race,
            soe_game::world::MenuRequest::PrecacheCharGenClear,
        ]
    );
}

/// RegisterForSleep delivers OnSleepStart and OnSleepStop when the player
/// sleeps and wakes, UnregisterForSleep stops them.
#[test]
fn sleep_events_reach_registered_listeners() {
    let mut g = game();
    g.start();
    let marker = obj(&mut g, "MarkerRef");
    g.world_mut().sleep_start(0.0, 0.5);
    g.run_until_idle(8);
    assert!(!g.traces().iter().any(|t| t == "sleep start"));
    call(&mut g, &marker, "RegisterForSleep", &[]);
    g.world_mut().sleep_start(0.0, 0.5);
    g.world_mut().sleep_stop(false);
    g.run_until_idle(8);
    assert!(g.traces().iter().any(|t| t == "sleep start"));
    assert!(g.traces().iter().any(|t| t == "sleep stop"));
    call(&mut g, &marker, "UnregisterForSleep", &[]);
    g.world_mut().sleep_start(1.0, 1.5);
    g.run_until_idle(8);
    assert_eq!(g.traces().iter().filter(|t| *t == "sleep start").count(), 1);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// RegisterForLOS delivers OnGainLOS for the viewer and target pair,
/// RegisterForSingleLOSGain once, UnregisterForLOS drops the pair.
#[test]
fn line_of_sight_events_reach_registered_listeners() {
    let mut g = game();
    g.start();
    let marker = obj(&mut g, "MarkerRef");
    let player = obj(&mut g, "PlayerRef");
    let guard = obj(&mut g, "GuardRef");
    let (p, gd) = (g.form("PlayerRef").unwrap(), g.form("GuardRef").unwrap());
    call(
        &mut g,
        &marker,
        "RegisterForSingleLOSGain",
        &[player.clone(), guard.clone()],
    );
    g.world_mut()
        .line_of_sight(p, gd, true, &mut |_, _| unreachable!());
    g.run_until_idle(8);
    g.world_mut()
        .line_of_sight(p, gd, true, &mut |_, _| unreachable!());
    g.run_until_idle(8);
    assert_eq!(g.traces().iter().filter(|t| *t == "los gained").count(), 1);
    call(
        &mut g,
        &marker,
        "RegisterForLOS",
        &[player.clone(), guard.clone()],
    );
    g.world_mut()
        .line_of_sight(p, gd, true, &mut |_, _| unreachable!());
    g.world_mut()
        .line_of_sight(p, gd, true, &mut |_, _| unreachable!());
    g.run_until_idle(8);
    assert_eq!(g.traces().iter().filter(|t| *t == "los gained").count(), 3);
    call(&mut g, &marker, "UnregisterForLOS", &[player, guard]);
    g.world_mut()
        .line_of_sight(p, gd, true, &mut |_, _| unreachable!());
    g.run_until_idle(8);
    assert_eq!(g.traces().iter().filter(|t| *t == "los gained").count(), 3);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- the dispatch walk -----

/// A script extending `ReferenceAlias` calls `GetReference` (declared
/// native on `ReferenceAlias.pex`, registered on `Alias`) and
/// `RegisterForSingleUpdate` (declared on `Alias.pex`, registered on
/// `Form`): both resolve by walking past the declaring class, the first
/// `start` on the installed game had left them unresolved.
#[test]
fn a_redeclared_native_resolves_on_the_parent_class() {
    let mut g = game();
    g.start();
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
    let guard = g.form("GuardRef").unwrap();
    let object = g.object_of_form(guard);
    // SCR-07: Debug.Trace carries the object's runtime form id alongside
    // its handle, not just the bare `Value::Object` display.
    let wanted = match g.world().form_of(object) {
        Some(id) => format!("[Object <{}> {:08X}]", object.0, g.world().runtime_id(id)),
        None => Value::Object(object).to_string(),
    };
    assert!(
        g.traces().contains(&wanted),
        "GetReference through the alias chain: {:?}",
        g.traces()
    );
    g.tick(2.0);
    assert!(
        g.traces().iter().any(|t| t == "watcher update"),
        "{:?}",
        g.traces()
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

// ----- quest, message, scene, sound, spell, game state -----

/// ModObjectiveGlobal "mods a global variable in a threadsafe way",
/// redisplays the objective and completes it once the target is reached,
/// returning whether it was. UpdateCurrentInstanceGlobal snapshots the
/// global. SetObjectiveDisplayedNoWait is SetObjectiveDisplayed.
#[test]
fn objective_globals_count_to_a_target() {
    let mut g = game();
    g.start();
    let quest = obj(&mut g, "LocQuest");
    let counter = obj(&mut g, "Counter");
    let args = |v: f32| {
        vec![
            Value::Float(v),
            counter.clone(),
            Value::Int(10),
            Value::Float(3.0),
            Value::Bool(true),
            Value::Bool(true),
            Value::Bool(true),
        ]
    };
    assert_eq!(
        call(&mut g, &quest, "ModObjectiveGlobal", &args(1.0)),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &quest, "IsObjectiveDisplayed", &[Value::Int(10)]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &quest, "IsObjectiveCompleted", &[Value::Int(10)]),
        Value::Bool(false)
    );
    assert_eq!(
        call(&mut g, &quest, "ModObjectiveGlobal", &args(2.0)),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &quest, "IsObjectiveCompleted", &[Value::Int(10)]),
        Value::Bool(true)
    );
    assert_eq!(call(&mut g, &counter, "GetValue", &[]), Value::Float(3.0));
    assert_eq!(
        call(&mut g, &quest, "UpdateCurrentInstanceGlobal", &[counter]),
        Value::Bool(true)
    );
    let (qid, cid) = (g.form("LocQuest").unwrap(), g.form("Counter").unwrap());
    assert_eq!(
        g.world().quest(qid).unwrap().journal.globals().unwrap()[&cid],
        3.0
    );
    call(
        &mut g,
        &quest,
        "SetObjectiveDisplayedNoWait",
        &[Value::Int(10), Value::Bool(false)],
    );
    assert_eq!(
        call(&mut g, &quest, "IsObjectiveDisplayed", &[Value::Int(10)]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Message.Show reports button 0 for a queued message box and -1 for a
/// notification, while ShowAsHelpMessage queues a notification.
#[test]
fn scr_66_messages_show_by_their_flag() {
    let mut g = game();
    let boxed = obj(&mut g, "BoxMessage");
    let note = obj(&mut g, "NoteMessage");
    assert_eq!(call(&mut g, &boxed, "Show", &[]), Value::Int(0));
    assert_eq!(call(&mut g, &note, "Show", &[]), Value::Int(-1));
    call(
        &mut g,
        &note,
        "ShowAsHelpMessage",
        &[
            Value::string("Help"),
            Value::Float(1.0),
            Value::Float(1.0),
            Value::Int(1),
        ],
    );
    assert_eq!(g.world().message_boxes, vec!["Hello box".to_string()]);
    assert_eq!(
        g.world().notifications,
        vec!["Hello note".to_string(), "Hello note".to_string()]
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn scr_36_reset_help_message_global_reenables_event_notification() {
    let mut g = game();
    let note = obj(&mut g, "NoteMessage");
    let args = [
        Value::string("Jump"),
        Value::Float(1.0),
        Value::Float(1.0),
        Value::Int(1),
    ];

    call(&mut g, &note, "ShowAsHelpMessage", &args);
    call(&mut g, &note, "ShowAsHelpMessage", &args);
    assert_eq!(g.world().notifications, vec!["Hello note"]);

    assert_eq!(
        call_global(
            &mut g,
            "Message",
            "ResetHelpMessage",
            &[Value::string("Jump")]
        ),
        Value::None
    );
    call(&mut g, &note, "ShowAsHelpMessage", &args);
    assert_eq!(g.world().notifications, vec!["Hello note", "Hello note"]);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn scr_36_reset_help_message_method_uses_event_argument() {
    let mut g = game();
    let note = obj(&mut g, "NoteMessage");
    let args = [Value::string("Activate")];

    call(&mut g, &note, "ShowAsHelpMessage", &args);
    assert_eq!(call(&mut g, &note, "ResetHelpMessage", &args), Value::None);
    call(&mut g, &note, "ShowAsHelpMessage", &args);
    assert_eq!(g.world().notifications, vec!["Hello note", "Hello note"]);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

#[test]
fn scr_36_reset_help_message_empty_event_is_a_noop() {
    let mut g = game();
    let note = obj(&mut g, "NoteMessage");
    let args = [Value::string("")];

    call(&mut g, &note, "ShowAsHelpMessage", &args);
    assert_eq!(
        call_global(&mut g, "Message", "ResetHelpMessage", &args),
        Value::None
    );
    assert!(g.world().notifications.is_empty());
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Scene.Start marks it playing and delivers OnBegin to the scene's
/// scripts, Stop delivers OnEnd, IsPlaying reads the flag.
#[test]
fn scenes_begin_and_end() {
    let mut g = game();
    g.start();
    let scene = obj(&mut g, "TestScene");
    assert_eq!(call(&mut g, &scene, "IsPlaying", &[]), Value::Bool(false));
    call(&mut g, &scene, "Start", &[]);
    assert_eq!(call(&mut g, &scene, "IsPlaying", &[]), Value::Bool(true));
    assert!(
        g.traces().iter().any(|t| t == "scene begin"),
        "{:?}",
        g.traces()
    );
    call(&mut g, &scene, "ForceStart", &[]);
    assert_eq!(g.traces().iter().filter(|t| *t == "scene begin").count(), 1);
    call(&mut g, &scene, "Stop", &[]);
    assert_eq!(call(&mut g, &scene, "IsPlaying", &[]), Value::Bool(false));
    assert!(g.traces().iter().any(|t| t == "scene end"));
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// scr_69_scene_scroll_registry_inventory_calls_each_vanilla_binding
#[test]
fn scr_69_scene_scroll_registry_inventory_calls_each_vanilla_binding() {
    let mut g = game();
    let scene = obj(&mut g, "TestScene");
    assert_eq!(call(&mut g, &scene, "IsPlaying", &[]), Value::Bool(false));
    call(&mut g, &scene, "Start", &[]);
    assert_eq!(call(&mut g, &scene, "IsPlaying", &[]), Value::Bool(true));
    assert_eq!(
        call(&mut g, &scene, "IsActionComplete", &[Value::Int(0)]),
        Value::Bool(false)
    );
    call(&mut g, &scene, "ForceStart", &[]);
    assert_eq!(call(&mut g, &scene, "GetOwningQuest", &[]), Value::None);
    call(&mut g, &scene, "Stop", &[]);
    assert_eq!(call(&mut g, &scene, "IsPlaying", &[]), Value::Bool(false));

    let scroll = obj(&mut g, "TestScroll");
    call(&mut g, &scroll, "Cast", &[Value::None, Value::None]);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// scr_69_scene_is_action_complete_reads_completed_action_slots
#[test]
fn scr_69_scene_is_action_complete_reads_completed_action_slots() {
    let mut g = game();
    let scene = obj(&mut g, "TestScene");
    call(&mut g, &scene, "Start", &[]);
    assert_eq!(
        call(&mut g, &scene, "IsActionComplete", &[Value::Int(0)]),
        Value::Bool(false)
    );
    let scene_id = g.form("TestScene").unwrap();
    assert!(g.world_mut().complete_scene_action(scene_id, 0));
    assert_eq!(
        call(&mut g, &scene, "IsActionComplete", &[Value::Int(0)]),
        Value::Bool(true)
    );
    assert_eq!(
        call(&mut g, &scene, "IsActionComplete", &[Value::Int(-1)]),
        Value::Bool(false)
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Sound.Play "plays the sound at the specified object's location" and
/// returns an instance id, SetInstanceVolume and StopInstance take it.
/// Spell.Cast records the cast with its source and target.
#[test]
fn sounds_and_spells_are_recorded() {
    let mut g = game();
    let sound = obj(&mut g, "TestSound");
    let guard = obj(&mut g, "GuardRef");
    let player = obj(&mut g, "PlayerRef");
    let first = call(&mut g, &sound, "Play", &[guard.clone()]);
    let second = call(&mut g, &sound, "Play", &[guard.clone()]);
    assert_eq!(first, Value::Int(1));
    assert_eq!(second, Value::Int(2));
    call_global(
        &mut g,
        "Sound",
        "SetInstanceVolume",
        &[Value::Int(2), Value::Float(0.25)],
    );
    assert_eq!(g.world().sound_instances[&2].2, 0.25);
    call_global(&mut g, "Sound", "StopInstance", &[Value::Int(1)]);
    assert!(!g.world().sound_instances.contains_key(&1));
    let flames = obj(&mut g, "Flames");
    call(&mut g, &flames, "Cast", &[guard.clone(), player.clone()]);
    call(&mut g, &flames, "RemoteCast", &[guard, player, Value::None]);
    let (f, gd, p) = (
        g.form("Flames").unwrap(),
        g.form("GuardRef").unwrap(),
        g.form("PlayerRef").unwrap(),
    );
    assert_eq!(g.world().spell_casts, vec![(f, gd, Some(p)), (f, gd, None)]);
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Spell.Preload and Spell.Unload validate the spell through the world API and
/// queue the matching host art request without changing magic state.
#[test]
fn scr_47_spell_art_methods_queue_ordered_host_requests() {
    let mut g = game();
    let flames = obj(&mut g, "Flames");
    let spell = g.form("Flames").unwrap();
    let spells_before = g.world().spell_casts.clone();
    let effects_before = g.world().active_effects().count();

    assert_eq!(call(&mut g, &flames, "Unload", &[]), Value::None);
    assert!(g.world_mut().take_spell_art_requests().is_empty());
    assert_eq!(call(&mut g, &flames, "Preload", &[]), Value::None);
    assert_eq!(call(&mut g, &flames, "Unload", &[]), Value::None);

    assert_eq!(
        g.world_mut().take_spell_art_requests(),
        vec![
            SpellArtRequest {
                spell,
                action: SpellArtAction::Preload
            },
            SpellArtRequest {
                spell,
                action: SpellArtAction::Unload
            },
        ]
    );
    assert_eq!(g.world().spell_casts, spells_before);
    assert_eq!(g.world().active_effects().count(), effects_before);
}

/// SCR-70 inventory: these are the registered ShaderParticleGeometry and
/// Sound methods and globals. Sound.psc also declares PlayAndWait and
/// GetDescriptor, which are not registered yet.
#[test]
fn scr_70_registry_matches_the_audited_sound_and_shader_bindings() {
    let g = game();
    let methods = [
        ("ShaderParticleGeometry", "Apply"),
        ("ShaderParticleGeometry", "Remove"),
        ("Sound", "Play"),
    ];
    for (class, name) in methods {
        assert!(g.vm.natives.has_method(class, name), "{class}.{name}");
        assert!(!g.vm.natives.has_global(class, name), "{class}.{name}");
    }
    let globals = [("Sound", "StopInstance"), ("Sound", "SetInstanceVolume")];
    for (class, name) in globals {
        assert!(g.vm.natives.has_global(class, name), "{class}.{name}");
        assert!(!g.vm.natives.has_method(class, name), "{class}.{name}");
    }
    assert!(!g.vm.natives.has_method("Sound", "PlayAndWait"));
    assert!(!g.vm.natives.has_method("Sound", "GetDescriptor"));
}

/// SCR-70 Sound.psc contract: Play returns a nonzero instance, volume is
/// clamped to 0 through 1, and StopInstance removes the selected instance.
#[test]
fn scr_70_sound_instances_follow_the_declared_contract() {
    let mut g = game();
    let sound = obj(&mut g, "TestSound");
    let source = obj(&mut g, "GuardRef");
    let first = call(&mut g, &sound, "Play", &[source.clone()]);
    let second = call(&mut g, &sound, "Play", &[source]);
    assert_eq!(first, Value::Int(1));
    assert_eq!(second, Value::Int(2));
    let sound_id = g.form("TestSound").unwrap();
    let source_id = g.form("GuardRef").unwrap();
    assert_eq!(g.world().sound_instances[&2], (sound_id, source_id, 1.0));

    call_global(
        &mut g,
        "Sound",
        "SetInstanceVolume",
        &[Value::Int(2), Value::Float(2.0)],
    );
    assert_eq!(g.world().sound_instances[&2].2, 1.0);
    call_global(
        &mut g,
        "Sound",
        "SetInstanceVolume",
        &[Value::Int(2), Value::Float(-1.0)],
    );
    assert_eq!(g.world().sound_instances[&2].2, 0.0);

    call_global(&mut g, "Sound", "StopInstance", &[Value::Int(1)]);
    assert!(!g.world().sound_instances.contains_key(&1));
}

/// The Game flags: fast travel, AI driven, chargen, beast form, words
/// (TeachWord then UnlockWord drive IsWordUnlocked), achievements, the
/// last ridden horse, and SetGameSetting* overriding GetGameSetting*.
#[test]
fn game_state_natives_round_trip() {
    let mut g = game();
    assert_eq!(
        call_global(&mut g, "Game", "IsFastTravelEnabled", &[]),
        Value::Bool(true)
    );
    call_global(&mut g, "Game", "EnableFastTravel", &[Value::Bool(false)]);
    assert_eq!(
        call_global(&mut g, "Game", "IsFastTravelEnabled", &[]),
        Value::Bool(false)
    );
    call_global(&mut g, "Game", "SetPlayerAIDriven", &[]);
    call_global(
        &mut g,
        "Game",
        "SetInChargen",
        &[Value::Bool(true), Value::Bool(true), Value::Bool(true)],
    );
    call_global(&mut g, "Game", "SetBeastForm", &[Value::Bool(true)]);
    assert!(g.world().player_ai_driven && !g.world().in_chargen && g.world().beast_form);
    assert!(g.world().chargen_disable_saving);
    assert!(g.world().chargen_disable_waiting);
    assert!(g.world().chargen_show_controls_disabled_message);
    let word = obj(&mut g, "WordFus");
    assert_eq!(
        call_global(&mut g, "Game", "IsWordUnlocked", &[word.clone()]),
        Value::Bool(false)
    );
    call_global(&mut g, "Game", "TeachWord", &[word.clone()]);
    assert_eq!(
        call_global(&mut g, "Game", "IsWordUnlocked", &[word.clone()]),
        Value::Bool(false)
    );
    call_global(&mut g, "Game", "UnlockWord", &[word.clone()]);
    assert_eq!(
        call_global(&mut g, "Game", "IsWordUnlocked", &[word]),
        Value::Bool(true)
    );
    call_global(&mut g, "Game", "AddAchievement", &[Value::Int(7)]);
    assert_eq!(g.world().achievements, vec![7]);
    assert_eq!(
        call_global(&mut g, "Game", "GetPlayersLastRiddenHorse", &[]),
        Value::None
    );
    call_global(
        &mut g,
        "Game",
        "SetGameSettingFloat",
        &[Value::string("fJumpHeightMin"), Value::Float(90.0)],
    );
    call_global(
        &mut g,
        "Game",
        "SetGameSettingInt",
        &[Value::string("iHoursToRespawnCell"), Value::Int(5)],
    );
    call_global(
        &mut g,
        "Game",
        "SetGameSettingBool",
        &[Value::string("bTest"), Value::Bool(true)],
    );
    call_global(
        &mut g,
        "Game",
        "SetGameSettingString",
        &[Value::string("sTest"), Value::string("x")],
    );
    assert_eq!(
        call_global(
            &mut g,
            "Game",
            "GetGameSettingFloat",
            &[Value::string("fJumpHeightMin")]
        ),
        Value::Float(90.0)
    );
    assert_eq!(
        call_global(
            &mut g,
            "Game",
            "GetGameSettingInt",
            &[Value::string("iHoursToRespawnCell")]
        ),
        Value::Int(5)
    );
    assert_eq!(
        call_global(
            &mut g,
            "Game",
            "GetGameSettingString",
            &[Value::string("sTest")]
        ),
        Value::string("x")
    );
    assert!(errors(&g).is_empty(), "{:?}", errors(&g));
}

/// Game presentation and save natives preserve their typed arguments and
/// enqueue ordered work for the owning host subsystems.
#[test]
fn scr_28_game_presentation_and_save_requests() {
    let mut g = game();
    for name in [
        "RequestSave",
        "RequestAutoSave",
        "ForceFirstPerson",
        "ForceThirdPerson",
    ] {
        assert_eq!(call_global(&mut g, "Game", name, &[]), Value::None);
    }
    assert_eq!(
        call_global(
            &mut g,
            "Game",
            "FadeOutGame",
            &[
                Value::Bool(true),
                Value::Bool(false),
                Value::Float(0.25),
                Value::Float(1.5),
            ],
        ),
        Value::None
    );
    assert_eq!(
        g.world_mut().take_host_requests(),
        vec![
            HostRequest::Save { automatic: false },
            HostRequest::Save { automatic: true },
            HostRequest::CameraFirstPerson,
            HostRequest::CameraThirdPerson,
            HostRequest::FadeOutGame {
                fading_out: true,
                black: false,
                seconds_before_fade: 0.25,
                fade_duration: 1.5,
            },
        ]
    );
    assert!(g.world_mut().take_host_requests().is_empty());
    assert_eq!(
        call_global(
            &mut g,
            "Game",
            "FadeOutGame",
            &[
                Value::Bool(false),
                Value::Bool(true),
                Value::Float(f32::NAN),
                Value::Float(1.0),
            ],
        ),
        Value::None
    );
    assert!(g.world_mut().take_host_requests().is_empty());
    assert!(g.vm.log.contains("Game.FadeOutGame"));
}

/// CK defines independent player-control flags and separate chargen flags.
/// The values survive the engine save through the SCR-60 world section.
#[test]
fn game_control_and_chargen_flags_follow_ck_and_save() {
    let mut g = game();
    call_global(
        &mut g,
        "Game",
        "DisablePlayerControls",
        &[
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Int(1),
        ],
    );
    assert_eq!(
        g.world().player_control_flags,
        [false, true, false, true, true, true, true, true]
    );
    assert!(!g.world().player_controls);
    assert_eq!(g.world().player_control_pov_type, 1);

    call_global(
        &mut g,
        "Game",
        "EnablePlayerControls",
        &[
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Int(0),
        ],
    );
    assert_eq!(
        g.world().player_control_flags,
        [true, true, true, true, true, true, true, true]
    );
    assert!(g.world().player_controls);
    assert_eq!(g.world().player_control_pov_type, 0);

    call_global(
        &mut g,
        "Game",
        "SetInChargen",
        &[Value::Bool(true), Value::Bool(true), Value::Bool(true)],
    );
    assert!(!g.world().in_chargen);
    assert!(g.world().chargen_disable_saving);
    assert!(g.world().chargen_disable_waiting);
    assert!(g.world().chargen_show_controls_disabled_message);

    let saved = g.world().save_state();
    g.world_mut().player_control_flags = [false; 8];
    g.world_mut().player_control_pov_type = 1;
    g.world_mut().chargen_disable_saving = false;
    g.world_mut().chargen_disable_waiting = false;
    g.world_mut().chargen_show_controls_disabled_message = false;
    g.world_mut().load_state(&saved).unwrap();
    assert_eq!(g.world().player_control_flags, [true; 8]);
    assert_eq!(g.world().player_control_pov_type, 0);
    assert!(g.world().chargen_disable_saving);
    assert!(g.world().chargen_disable_waiting);
    assert!(g.world().chargen_show_controls_disabled_message);
}

/// Everything above survives a save and load: the leveled list addition,
/// the faction bounty, the perk, the location flag and the listeners.
#[test]
fn the_new_state_survives_save_and_load() {
    let mut g = game();
    g.start();
    let list = obj(&mut g, "LootList");
    let flames = obj(&mut g, "Flames");
    call(
        &mut g,
        &list,
        "AddForm",
        &[flames, Value::Int(20), Value::Int(3)],
    );
    let guards = obj(&mut g, "GuardsFaction");
    call(
        &mut g,
        &guards,
        "ModCrimeGold",
        &[Value::Int(40), Value::Bool(false)],
    );
    let inn = obj(&mut g, "InnLoc");
    call(&mut g, &inn, "SetCleared", &[]);
    let marker = obj(&mut g, "MarkerRef");
    call(&mut g, &marker, "RegisterForSleep", &[]);
    call_global(
        &mut g,
        "Game",
        "IncrementStat",
        &[Value::string("Barters"), Value::Int(4)],
    );
    let bytes = g.save_state().unwrap();

    let mut h = game();
    h.start();
    copy_probe_scripts(&g, &mut h);

    h.load_state(&bytes).unwrap();
    let list = obj(&mut h, "LootList");
    assert_eq!(call(&mut h, &list, "GetNumForms", &[]), Value::Int(3));
    let guards = obj(&mut h, "GuardsFaction");
    assert_eq!(call(&mut h, &guards, "GetCrimeGold", &[]), Value::Int(40));
    let inn = obj(&mut h, "InnLoc");
    assert_eq!(call(&mut h, &inn, "IsCleared", &[]), Value::Bool(true));
    assert_eq!(
        call_global(&mut h, "Game", "QueryStat", &[Value::string("Barters")]),
        Value::Int(4)
    );
    h.world_mut().sleep_start(0.0, 0.5);
    h.run_until_idle(8);
    assert!(
        h.traces().iter().any(|t| t == "sleep start"),
        "{:?}",
        h.traces()
    );
    assert!(errors(&h).is_empty(), "{:?}", errors(&h));
}

/// A held reference whose materialized identity disagrees with its stack must
/// fail preflight before an earlier FormList base member can be added.
#[test]
fn add_item_preflights_inconsistent_held_reference_before_base_mutation() {
    for actor_mismatch in [false, true] {
        let mut g = game();
        let guard = g.form("GuardRef").unwrap();
        let source = g.form("MarkerRef").unwrap();
        let gold = g.form("Gold001").unwrap();
        let sword = g.form("IronSword").unwrap();
        let loose = g.form("LooseSwordRef").unwrap();
        let list = g.form("ReferenceAddList").unwrap();
        let shared = g.shared_world();
        let mut world = shared.borrow_mut();
        let mut alloc = |name: &str, class: &str| g.vm.create_object_of(name, class);
        assert!(world.add_item_notified(source, loose, 1, &mut alloc));
        world.pending.clear();
        let state = world.reference(loose).unwrap();
        if actor_mismatch {
            state.is_actor = true;
        } else {
            state.base = Some(gold);
        }
        assert!(!world.add_form_list_notified(guard, list, 1, &mut alloc));
        assert_eq!(world.item_count(guard, gold), 0);
        assert_eq!(world.item_count(guard, sword), 0);
        assert_eq!(world.item_count(source, sword), 2);
        assert!(world.pending.is_empty());
    }
}
