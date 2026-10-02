// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::detect_life::presentations;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn mag_40_host_presents_selected_actor_with_authored_shader_in_godot_coordinates() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    for (form, position) in [(0x100, [0.0_f32, 0.0, 0.0]), (0x101, [100.0, 200.0, 300.0])] {
        let mut data = vec![0; 24];
        for (axis, value) in position.into_iter().enumerate() {
            data[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(
            Record::new(T(b"ACHR"), form)
                .sub(T(b"NAME"), 0x500u32.to_le_bytes())
                .sub(T(b"DATA"), data),
        );
    }
    let mut data = vec![0; 152];
    put(&mut data, 32, 0x400);
    put(&mut data, 64, 19);
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), data));
    plugin.push_top(Record::new(T(b"EFSH"), 0x400));
    let mut spit = vec![0; 36];
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x300)
            .sub(T(b"SPIT"), std::mem::take(&mut spit))
            .sub(T(b"EFID"), 0x200u32.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    500.0f32.to_le_bytes().to_vec(),
                    0u32.to_le_bytes().to_vec(),
                    5u32.to_le_bytes().to_vec(),
                ]
                .concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("DetectLife.esm", plugin).unwrap();
    let mut game = Game::new(order);
    for actor in [0x100, 0x101] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
        game.world_mut().reference(id(actor)).unwrap();
    }
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

    let mut world = game.world_mut();
    let output = presentations(&mut world).unwrap();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].actor, id(0x101));
    assert_eq!(
        output[0].position.x,
        100.0 * soe_godot_host::units::METRES_PER_UNIT
    );
    assert_eq!(
        output[0].position.y,
        300.0 * soe_godot_host::units::METRES_PER_UNIT
    );
    assert_eq!(
        output[0].position.z,
        -200.0 * soe_godot_host::units::METRES_PER_UNIT
    );
    assert_eq!(output[0].hit_shader, Some(id(0x400)));
}
