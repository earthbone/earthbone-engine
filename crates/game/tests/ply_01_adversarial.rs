// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::Hand;
use soe_game::player_input::{Command, Driver, STEP_SECONDS};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const START: u32 = 0x800;
const DESTINATION: u32 = 0x801;
const MARKER: u32 = 0x810;
const BASE: u32 = 0x811;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"CELL"), START));
    plugin.push_top(Record::new(T(b"CELL"), DESTINATION));
    plugin.push_top(Record::new(T(b"MISC"), BASE));
    plugin.push_top(
        Record::new(T(b"REFR"), MARKER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(
                T(b"DATA"),
                Transform {
                    position: [10.0, 20.0, 30.0],
                    rotation: [1.1, 1.2, 1.3],
                }
                .to_bytes(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Input.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    let mut world = game.world_mut();
    let player_ref = world.reference(player).unwrap();
    player_ref.cell = Some(id(START));
    player_ref.rotation = [0.1, 0.2, 0.3];
    let marker = world.reference(id(MARKER)).unwrap();
    marker.cell = Some(id(DESTINATION));
    marker.position = [10.0, 20.0, 30.0];
    marker.rotation = [1.1, 1.2, 1.3];
    drop(world);
    game.change_cell(Some(id(START)));
    game
}

#[test]
fn ply_01_move_to_an_unloaded_cell_preserves_rotation() {
    let mut game = fixture();
    let mut driver = Driver::default();
    driver
        .push(Command::MoveTo {
            target: id(MARKER),
            offset: [0.0; 3],
            match_rotation: true,
        })
        .unwrap();
    let event = driver.step(&mut game).unwrap();
    assert!(event.result.is_ok(), "{event:?}");
    assert_eq!(event.cell, Some(id(DESTINATION)));
    assert_eq!(event.position, [10.0, 20.0, 30.0]);
    assert_eq!(event.rotation, [0.1, 0.2, 0.3]);
}

#[test]
fn ply_01_missing_move_target_rejects_without_changing_player_pose() {
    let mut game = fixture();
    let before = {
        let player = game.player();
        let world = game.world();
        let state = world.reference_ref(player).unwrap();
        (state.cell, state.position, state.rotation)
    };
    let mut driver = Driver::default();
    driver
        .push(Command::MoveTo {
            target: id(0x8fff),
            offset: [5.0, 6.0, 7.0],
            match_rotation: true,
        })
        .unwrap();
    let event = driver.step(&mut game).unwrap();
    assert!(
        event.result.is_err(),
        "missing reference must reject: {event:?}"
    );
    assert_eq!((event.cell, event.position, event.rotation), before);
}

#[test]
fn ply_01_export_shared_sequence_for_windowed_godot_comparison() {
    let path = std::env::var_os("SOE_PLY_01_FIXTURE").expect("fixture output path is required");
    let path = std::path::PathBuf::from(path);
    super::ply_01_player_input::export_window_fixture(&path);
    assert!(path.join("expected.log").is_file());
}

#[test]
fn ply_01_attack_and_cast_commands_reach_shared_combat_and_magic_paths() {
    let mut game = super::ply_01_player_input::game();
    let player = game.player();
    let speaker = id(super::ply_01_player_input::SPEAKER);
    let weapon = id(super::ply_01_player_input::WEAPON);
    let spell = id(super::ply_01_player_input::SPELL);
    for actor in [player, speaker] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.0)
            .unwrap();
    }
    {
        let mut world = game.world_mut();
        world.add_item(player, weapon, 1);
        world.actor(player).unwrap().equipped.insert(weapon);
        world.set_actor_value(player, "magicka", 100.0).unwrap();
        world.actor(player).unwrap().spells.insert(spell);
        world.equip_hand_spell(player, Hand::Right, spell).unwrap();
    }
    let mut driver = Driver::default();
    for command in [
        Command::Attack {
            target: speaker,
            weapon,
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
    ] {
        driver.push(command).unwrap();
    }
    let start = game.world().clock.real_seconds;
    let mut step = 0;
    while let Some(event) = driver.step(&mut game) {
        assert!(event.result.is_ok(), "{event:?}");
        assert_eq!(event.step, step);
        step += 1;
    }
    assert_eq!(step, 5);
    assert!((game.world().clock.real_seconds - start - 5.0 * STEP_SECONDS).abs() < 1e-12);
    assert!(game.world_mut().actor_value(speaker, "health") < 100.0);
    assert_eq!(game.world_mut().actor_value(player, "magicka"), 80.0);
}
