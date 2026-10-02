// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 0x100).sub(Tag::new(b"ACBS"), vec![0; 24]));
    for actor in [0x101, 0x102] {
        plugin.push_top(
            Record::new(Tag::new(b"ACHR"), actor)
                .sub(Tag::new(b"NAME"), 0x100u32.to_le_bytes())
                .sub(Tag::new(b"DATA"), vec![0; 24]),
        );
    }
    let mut effect = vec![0; 152];
    effect[64..68].copy_from_slice(&26u32.to_le_bytes());
    effect[80..84].copy_from_slice(&1u32.to_le_bytes());
    effect[84..88].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(Record::new(Tag::new(b"MGEF"), 0x200).sub(Tag::new(b"DATA"), effect));
    let mut spell = vec![0; 36];
    spell[16..20].copy_from_slice(&1u32.to_le_bytes());
    spell[20..24].copy_from_slice(&3u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag::new(b"SPEL"), 0x300)
            .sub(Tag::new(b"SPIT"), spell)
            .sub(Tag::new(b"EFID"), 0x200u32.to_le_bytes())
            .sub(
                Tag::new(b"EFIT"),
                [1.0f32.to_le_bytes().as_slice(), &[0; 8]].concat(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Feed.esp", plugin).unwrap();
    let game = Game::new(order);
    for actor in [0x101, 0x102] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.0)
            .unwrap();
    }
    game
}

#[test]
fn mag_47_feed_consumes_corpse_once_and_rejects_a_repeated_cast() {
    let mut game = fixture();
    game.world_mut().actor(id(0x102)).unwrap().dead = true;
    let result = game.shared_world().borrow_mut().cast_spell(
        id(0x300),
        id(0x101),
        Some(id(0x101)),
        Some(id(0x102)),
        &mut |name, class| game.vm.create_object_of(name, class),
    );
    assert!(result.is_ok());
    {
        let world = game.world();
        let corpse = world.reference_ref(id(0x102)).unwrap();
        assert!(corpse.deleted && !corpse.enabled);
    }
    assert!(
        game.shared_world()
            .borrow_mut()
            .cast_spell(
                id(0x300),
                id(0x101),
                Some(id(0x101)),
                Some(id(0x102)),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .is_err()
    );
}

#[test]
fn mag_47_feed_does_not_consume_a_living_target_or_other_corpse() {
    let mut game = fixture();
    game.world_mut().actor(id(0x102)).unwrap().dead = true;
    assert!(
        game.shared_world()
            .borrow_mut()
            .cast_spell(
                id(0x300),
                id(0x101),
                Some(id(0x101)),
                Some(id(0x101)),
                &mut |name, class| game.vm.create_object_of(name, class),
            )
            .is_err()
    );
    {
        let world = game.world();
        let corpse = world.reference_ref(id(0x102)).unwrap();
        assert!(!corpse.deleted && corpse.enabled);
    }
}
