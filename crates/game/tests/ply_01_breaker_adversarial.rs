// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::Transform;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::player_input::{Command, Driver, Error, STEP_SECONDS};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x800));
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x801));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x811));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 0x810)
            .sub(Tag(*b"NAME"), 0x811u32.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                Transform {
                    position: [10.0, 20.0, 30.0],
                    rotation: [1.0, 2.0, 3.0],
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
    player_ref.cell = Some(id(0x800));
    player_ref.rotation = [0.1, 0.2, 0.3];
    let target = world.reference(id(0x810)).unwrap();
    target.cell = Some(id(0x801));
    drop(world);
    game.change_cell(Some(id(0x800)));
    game
}

#[test]
fn ply_01_breaker_rejected_input_still_consumes_one_fixed_step() {
    let mut game = fixture();
    let start = game.world().clock.real_seconds;
    let mut driver = Driver::default();
    driver
        .push(Command::Activate { target: id(0x8fff) })
        .unwrap();
    let event = driver.step(&mut game).unwrap();
    assert!(event.result.is_err(), "missing activation must reject: {event:?}");
    assert_eq!(event.step, 0);
    assert!((game.world().clock.real_seconds - start - STEP_SECONDS).abs() < 1e-12);
    assert_eq!(driver.pending(), 0);
}

#[test]
fn ply_01_breaker_move_to_without_rotation_matching_keeps_player_facing() {
    let mut game = fixture();
    let mut driver = Driver::default();
    driver
        .push(Command::MoveTo {
            target: id(0x810),
            offset: [1.0, -2.0, 3.0],
            match_rotation: false,
        })
        .unwrap();
    let event = driver.step(&mut game).unwrap();
    assert!(event.result.is_ok(), "{event:?}");
    assert_eq!(event.cell, Some(id(0x801)));
    assert_eq!(event.position, [11.0, 18.0, 33.0]);
    assert_eq!(event.rotation, [0.1, 0.2, 0.3]);
}

#[test]
fn ply_01_breaker_non_finite_transform_is_rejected_before_queueing() {
    let mut driver = Driver::default();
    let result = driver.push(Command::Turn {
        rotation: [0.0, f32::NAN, 0.0],
    });
    assert!(matches!(result, Err(Error::Transform)));
    assert_eq!(driver.pending(), 0);
}
