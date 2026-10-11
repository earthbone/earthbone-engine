// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn fixture(area: u32, magnitude: f32, shader: u32, positions: &[[f32; 3]]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for (index, position) in positions.iter().enumerate() {
        let actor = 0x100 + index as u32;
        let mut data = vec![0; 24];
        for (axis, value) in position.iter().enumerate() {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), actor)
                .sub(T(b"NAME"), 0x500u32.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    let mut data = vec![0; 152];
    put(&mut data, 32, shader);
    put(&mut data, 64, 19);
    put(&mut data, 80, 2);
    let mut spit = vec![0; 36];
    put(&mut spit, 16, 2);
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), data));
    if shader != 0 {
        plugin.push_top(Record::new(T(b"EFSH"), shader));
    }
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x300)
            .sub(T(b"SPIT"), spit)
            .sub(T(b"EFID"), 0x200u32.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    magnitude.to_le_bytes().to_vec(),
                    area.to_le_bytes().to_vec(),
                    5u32.to_le_bytes().to_vec(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("DetectLife.esm", plugin).unwrap();
    let game = Game::new(order);
    for index in 0..positions.len() {
        game.world_mut()
            .set_actor_value(id(0x100 + index as u32), "health", 100.0)
            .unwrap();
        game.world_mut()
            .reference(id(0x100 + index as u32))
            .unwrap();
    }
    game
}

fn cast(game: &mut Game) {
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(0x300),
            id(0x100),
            Some(id(0x100)),
            None,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
}

#[test]
fn mag_40_area_range_uses_three_dimensional_distance_and_includes_boundary() {
    let mut game = fixture(
        200,
        0.0,
        0x400,
        &[[0.0, 0.0, 0.0], [0.0, 0.0, 200.0], [160.0, 160.0, 0.0]],
    );
    cast(&mut game);

    let targets = game.world_mut().detect_life_targets().unwrap();

    assert!(targets.iter().any(|target| target.actor == id(0x101)));
    assert!(
        targets.iter().all(|target| target.actor != id(0x102)),
        "off-axis actor outside area was selected: {targets:?}"
    );
}

#[test]
fn mag_40_zero_area_and_zero_magnitude_selects_no_targets() {
    let mut game = fixture(0, 0.0, 0x400, &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]]);
    cast(&mut game);

    assert!(game.world_mut().detect_life_targets().unwrap().is_empty());
}

#[test]
fn mag_40_null_authored_hit_shader_keeps_detection_without_a_shader() {
    let mut game = fixture(0, 100.0, 0, &[[0.0, 0.0, 0.0], [50.0, 0.0, 0.0]]);
    cast(&mut game);

    let targets = game.world_mut().detect_life_targets().unwrap();

    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].actor, id(0x101));
    assert_eq!(targets[0].hit_shader, None);
}
