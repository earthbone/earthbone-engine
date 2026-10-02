// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PLY-01 shared commands, timing and observable Papyrus interaction traces.
//! The optional fixture export feeds the windowed Godot comparison.

use crate::common::installed;
use crate::support::magic_adversarial::{effect, spell};
use soe_esm::records::Transform;
use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::Hand;
use soe_game::player_input::{Command, Confirmation, Driver, Error, STEP_SECONDS};
use soe_game::vmad::{InfoFragment, InfoFragments, ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallStatic, Return};
use std::fmt::Write;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL_A: u32 = 0x800;
const CELL_B: u32 = 0x801;
const DOOR: u32 = 0x810;
const LOAD_DOOR: u32 = 0x811;
const FAR_DOOR: u32 = 0x812;
const ITEM_REF: u32 = 0x820;
const ITEM: u32 = 0x821;
pub(super) const SPEAKER: u32 = 0x830;
const QUEST: u32 = 0x840;
const TOPIC: u32 = 0x841;
const INFO: u32 = 0x842;
pub(super) const WEAPON: u32 = 0x850;
pub(super) const SPELL: u32 = 0x860;
const EFFECT: u32 = 0x861;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn placement(object: u32, base: u32, position: [f32; 3], actor: bool) -> Record {
    Record::new(T(if actor { b"ACHR" } else { b"REFR" }), object)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(
            T(b"DATA"),
            Transform {
                position,
                rotation: [0.0; 3],
            }
            .to_bytes(),
        )
}

fn scripted(record: Record, script: &str) -> Record {
    let tag = record.tag;
    record.sub(
        T(b"VMAD"),
        Vmad {
            scripts: vec![ScriptEntry::new(script)],
            ..Vmad::default()
        }
        .write(tag),
    )
}

fn cell(plugin: &mut Plugin, object: u32, name: &str, references: Vec<Record>) {
    let index = plugin.push_record(
        Record::new(T(b"CELL"), object)
            .zsub(T(b"EDID"), name)
            .sub(T(b"DATA"), [1, 0]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(object));
    for reference in references {
        temporary
            .entries
            .push(Entry::Record(plugin.push_record(reference)));
    }
    let mut children = Group::new(GroupKind::CellChildren(object));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(index));
    sub.entries.push(Entry::Group(children));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
}

pub(super) fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    for base in [7, 0x900] {
        plugin.push_top(Record::new(T(b"NPC_"), base).sub(T(b"ACBS"), vec![0; 24]));
    }
    plugin.push_top(Record::new(T(b"DOOR"), 0x901));
    plugin.push_top(Record::new(T(b"MISC"), ITEM).sub(T(b"DATA"), vec![0; 8]));
    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&10u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&(-1i32).to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1i32).to_le_bytes());
    plugin.push_top(
        Record::new(T(b"WEAP"), WEAPON)
            .sub(T(b"DATA"), data)
            .sub(T(b"DNAM"), dnam),
    );
    let mut magic_effect = effect(EFFECT, 0, 1 << 1, 24);
    let data = &mut magic_effect
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"DATA"))
        .unwrap()
        .data;
    data[84..88].copy_from_slice(&0u32.to_le_bytes());
    plugin.push_top(magic_effect);
    let mut magic_spell = spell(SPELL, EFFECT, 5, 2.0, 1);
    let data = &mut magic_spell
        .subrecords
        .iter_mut()
        .find(|sub| sub.tag == T(b"SPIT"))
        .unwrap()
        .data;
    data[0..4].copy_from_slice(&10u32.to_le_bytes());
    data[4..8].copy_from_slice(&1u32.to_le_bytes());
    data[20..24].copy_from_slice(&0u32.to_le_bytes());
    plugin.push_top(magic_spell);
    let mut teleport = FAR_DOOR.to_le_bytes().to_vec();
    teleport.extend_from_slice(
        &Transform {
            position: [100.0, 0.0, 0.0],
            rotation: [0.0; 3],
        }
        .to_bytes(),
    );
    teleport.extend_from_slice(&0u32.to_le_bytes());
    cell(
        &mut plugin,
        CELL_A,
        "PlyStart",
        vec![
            placement(0x14, 7, [0.0; 3], true),
            scripted(
                placement(DOOR, 0x901, [10.0, 0.0, 0.0], false),
                "DoorEvents",
            ),
            placement(LOAD_DOOR, 0x901, [20.0, 0.0, 0.0], false).sub(T(b"XTEL"), teleport),
        ],
    );
    cell(
        &mut plugin,
        CELL_B,
        "PlyDestination",
        vec![
            placement(FAR_DOOR, 0x901, [100.0, 0.0, 0.0], false),
            scripted(
                placement(ITEM_REF, ITEM, [110.0, 0.0, 0.0], false),
                "ItemEvents",
            ),
            scripted(
                placement(SPEAKER, 0x900, [120.0, 0.0, 0.0], true),
                "SpeakerEvents",
            ),
        ],
    );
    plugin.push_top(soe_game::quest::build_record(QUEST, "PlyQuest", 1));
    plugin.push_top(
        Record::new(T(b"DLBR"), 0x843)
            .sub(T(b"QNAM"), QUEST.to_le_bytes())
            .sub(T(b"TNAM"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 1u32.to_le_bytes())
            .sub(T(b"SNAM"), TOPIC.to_le_bytes()),
    );
    let topic = Record::new(T(b"DIAL"), TOPIC)
        .zsub(T(b"FULL"), "A question")
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), 0x843u32.to_le_bytes())
        .sub(T(b"DATA"), [0; 4])
        .sub(T(b"SNAM"), *b"CUST");
    let mut response = vec![0; 24];
    response[12] = 1;
    let fragment = |function: &str| InfoFragment {
        marker: 1,
        script: "DialogueEvents".into(),
        function: function.into(),
    };
    let vmad = Vmad {
        scripts: vec![ScriptEntry::new("DialogueEvents")],
        info_fragments: Some(InfoFragments {
            version: 2,
            file: "DialogueEvents".into(),
            begin: Some(fragment("Fragment_0")),
            end: Some(fragment("Fragment_1")),
        }),
        ..Vmad::default()
    };
    let info = Record::new(T(b"INFO"), INFO)
        .sub(T(b"ENAM"), [0; 4])
        .sub(T(b"TRDT"), response)
        .zsub(T(b"NAM1"), "Hello from the shared runtime")
        .sub(T(b"VMAD"), vmad.write(T(b"INFO")));
    let mut group = Group::new(GroupKind::Top(T(b"DIAL")));
    group.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    group.entries.push(Entry::Group(children));
    plugin.groups.push(group);
    plugin
}

fn trace(function: &str, message: &str, parameters: &[(&str, &str)]) -> soe_papyrus::pex::Function {
    let mut function = FunctionBuilder::new(function, "None").local("unused", "None");
    for &(name, kind) in parameters {
        function = function.param(name, kind);
    }
    function
        .ins(
            CallStatic,
            vec![s("Debug"), s("Trace"), ident("unused"), s(message), i(0)],
        )
        .ins(Return, vec![none()])
        .build()
}

pub(super) fn scripts() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "Debug",
            ScriptBuilder::new("Debug", "")
                .function(
                    FunctionBuilder::new("Trace", "None")
                        .global()
                        .native()
                        .param("message", "String")
                        .param("severity", "Int")
                        .build(),
                )
                .bytes(),
        ),
        (
            "DoorEvents",
            ScriptBuilder::new("DoorEvents", "ObjectReference")
                .function(trace(
                    "OnActivate",
                    "door activated",
                    &[("activator", "ObjectReference")],
                ))
                .bytes(),
        ),
        (
            "ItemEvents",
            ScriptBuilder::new("ItemEvents", "ObjectReference")
                .function(trace(
                    "OnActivate",
                    "item activated",
                    &[("activator", "ObjectReference")],
                ))
                .function(trace(
                    "OnContainerChanged",
                    "item picked up",
                    &[("new", "ObjectReference"), ("old", "ObjectReference")],
                ))
                .bytes(),
        ),
        (
            "SpeakerEvents",
            ScriptBuilder::new("SpeakerEvents", "Actor")
                .function(trace(
                    "OnActivate",
                    "speaker activated",
                    &[("activator", "ObjectReference")],
                ))
                .bytes(),
        ),
        (
            "DialogueEvents",
            ScriptBuilder::new("DialogueEvents", "TopicInfo")
                .function(trace(
                    "Fragment_0",
                    "topic begin",
                    &[("speaker", "ObjectReference")],
                ))
                .function(trace(
                    "Fragment_1",
                    "topic end",
                    &[("speaker", "ObjectReference")],
                ))
                .bytes(),
        ),
    ]
}

pub(super) fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&plugin().to_bytes()).unwrap())
        .unwrap();
    order.build_cell_index();
    let mut game = Game::new(order);
    for (_, bytes) in scripts() {
        game.load_script(&bytes).unwrap();
    }
    game.start();
    let player = game.player();
    game.world_mut().reference(player).unwrap().cell = Some(id(CELL_A));
    game.change_cell(Some(id(CELL_A)));
    game
}

fn sequence() -> Vec<Command> {
    vec![
        Command::MoveTo {
            target: id(DOOR),
            offset: [0.0, 5.0, 0.0],
            match_rotation: true,
        },
        Command::Turn {
            rotation: [0.0, 0.0, 1.0],
        },
        Command::Activate { target: id(DOOR) },
        Command::Activate {
            target: id(LOAD_DOOR),
        },
        Command::MoveTo {
            target: id(ITEM_REF),
            offset: [0.0; 3],
            match_rotation: false,
        },
        Command::Activate {
            target: id(ITEM_REF),
        },
        Command::MoveTo {
            target: id(SPEAKER),
            offset: [0.0, 5.0, 0.0],
            match_rotation: true,
        },
        Command::Activate {
            target: id(SPEAKER),
        },
        Command::ChooseTopic { topic: id(TOPIC) },
        Command::ConfirmMenu(Confirmation::Dialogue),
        Command::Wait,
    ]
}

fn run(game: &mut Game, commands: Vec<Command>) -> Driver {
    let mut driver = Driver::default();
    for command in commands {
        driver.push(command).unwrap();
    }
    while let Some(event) = driver.step(game) {
        assert!(event.result.is_ok(), "{event:?}");
    }
    driver
}

pub(super) fn export_window_fixture(path: &std::path::Path) {
    let mut game = game();
    let driver = run(&mut game, sequence());
    std::fs::create_dir_all(path.join("scripts")).unwrap();
    std::fs::write(path.join("Skyrim.esm"), plugin().to_bytes()).unwrap();
    for name in [
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        std::fs::write(
            path.join(name),
            Plugin::new(FileHeader::default()).to_bytes(),
        )
        .unwrap();
    }
    for (name, bytes) in scripts() {
        std::fs::write(path.join("scripts").join(format!("{name}.pex")), bytes).unwrap();
    }
    let mut log = String::new();
    for event in driver.events() {
        writeln!(&mut log, "{event:?}").unwrap();
    }
    std::fs::write(path.join("expected.log"), log).unwrap();
}

#[test]
fn ply_01_sequence_opens_door_picks_up_item_and_starts_conversation_with_ordered_events() {
    let mut game = game();
    let driver = run(&mut game, sequence());
    assert!(game.world().reference_ref(id(DOOR)).unwrap().opened);
    let player = game.player();
    assert_eq!(game.world_mut().item_count(player, id(ITEM)), 1);
    assert_eq!(
        game.world().dialogue_session().unwrap().speaker,
        id(SPEAKER)
    );
    assert_eq!(game.world().loaded_cell(), Some(id(CELL_B)));
    let traces: Vec<_> = driver
        .events()
        .iter()
        .flat_map(|event| event.traces.clone())
        .collect();
    assert_eq!(
        traces,
        [
            "door activated",
            "item picked up",
            "item activated",
            "speaker activated",
            "topic begin",
            "topic end"
        ]
    );
    assert_eq!(driver.events(), run(&mut self::game(), sequence()).events());
    if let Some(path) = std::env::var_os("SOE_PLY_01_FIXTURE") {
        export_window_fixture(&std::path::PathBuf::from(path));
    }
}

#[test]
fn ply_01_fixed_steps_move_turn_and_wait_without_idle_clock_drift() {
    let mut game = game();
    let start = game.world().clock.real_seconds;
    let mut driver = run(
        &mut game,
        vec![
            Command::MoveTo {
                target: id(DOOR),
                offset: [2.0, 3.0, 4.0],
                match_rotation: false,
            },
            Command::Turn {
                rotation: [0.25, 0.5, 1.0],
            },
            Command::Wait,
        ],
    );
    assert_eq!(driver.events()[2].position, [12.0, 3.0, 4.0]);
    assert_eq!(driver.events()[2].rotation, [0.25, 0.5, 1.0]);
    assert!((game.world().clock.real_seconds - start - 3.0 * STEP_SECONDS).abs() < 1e-12);
    let clock = game.world().clock.real_seconds;
    assert!(driver.step(&mut game).is_none());
    assert_eq!(game.world().clock.real_seconds, clock);
}

#[test]
fn ply_01_attack_and_cast_use_shared_damage_magicka_and_hand_release() {
    let mut game = game();
    let player = game.player();
    for actor in [player, id(SPEAKER)] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.0)
            .unwrap();
    }
    {
        let mut world = game.world_mut();
        world.add_item(player, id(WEAPON), 1);
        world.actor(player).unwrap().equipped.insert(id(WEAPON));
        world.set_actor_value(player, "magicka", 100.0).unwrap();
        world.actor(player).unwrap().spells.insert(id(SPELL));
        world
            .equip_hand_spell(player, Hand::Right, id(SPELL))
            .unwrap();
    }
    run(
        &mut game,
        vec![
            Command::Attack {
                target: id(SPEAKER),
                weapon: id(WEAPON),
                power: false,
            },
            Command::Cast {
                hand: Hand::Right,
                target: None,
                held: true,
            },
            Command::Cast {
                hand: Hand::Right,
                target: None,
                held: true,
            },
            Command::Cast {
                hand: Hand::Right,
                target: None,
                held: false,
            },
            Command::Cast {
                hand: Hand::Right,
                target: None,
                held: true,
            },
        ],
    );
    assert!(game.world_mut().actor_value(id(SPEAKER), "health") < 100.0);
    assert_eq!(game.world_mut().actor_value(player, "magicka"), 80.0);
    assert_eq!(game.world().active_effects().count(), 2);
}

#[test]
fn ply_01_invalid_transforms_blocked_activation_and_repeat_pickup_are_observable() {
    let mut game = game();
    let mut driver = Driver::default();
    assert!(matches!(
        driver
            .push(Command::Turn {
                rotation: [f32::NAN, 0.0, 0.0]
            })
            .unwrap_err(),
        Error::Transform
    ));
    assert!(matches!(
        driver
            .push(Command::MoveTo {
                target: id(DOOR),
                offset: [f32::INFINITY; 3],
                match_rotation: true
            })
            .unwrap_err(),
        Error::Transform
    ));
    assert_eq!(driver.pending(), 0);
    game.world_mut()
        .reference(id(DOOR))
        .unwrap()
        .activation_blocked = true;
    driver.push(Command::Activate { target: id(DOOR) }).unwrap();
    let event = driver.step(&mut game).unwrap();
    assert!(event.result.is_err());
    assert_eq!(event.traces, ["door activated"]);
    assert!(!game.world().reference_ref(id(DOOR)).unwrap().opened);
    run(
        &mut game,
        vec![Command::Activate {
            target: id(ITEM_REF),
        }],
    );
    driver
        .push(Command::Activate {
            target: id(ITEM_REF),
        })
        .unwrap();
    assert!(driver.step(&mut game).unwrap().result.is_err());
    let player = game.player();
    assert_eq!(game.world_mut().item_count(player, id(ITEM)), 1);
}

#[test]
fn ply_01_installed_vanilla_door_uses_the_same_input_layer() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let cell = game.world().form("WhiterunBanneredMare").unwrap();
    let player = game.player();
    game.world_mut().reference(player).unwrap().cell = Some(cell);
    game.change_cell(Some(cell));
    let doors = game
        .world()
        .order()
        .build_cell_index()
        .references(cell)
        .to_vec();
    let target = doors
        .into_iter()
        .find(|reference| {
            let mut world = game.world_mut();
            let Some(reference) = world.reference(*reference) else {
                return false;
            };
            reference.teleport.is_some()
                && !reference.locked
                && reference.enabled
                && !reference.deleted
        })
        .expect("vanilla Bannered Mare load door");
    let driver = run(&mut game, vec![Command::Activate { target }]);
    assert_ne!(driver.events()[0].cell, Some(cell));
}
