// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::world::{PlayerCharacter, ProcessLists};
use soe_commonlib_shim::world_runtime::WorldState;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, LoadOrder, Tag};
use soe_game::Game;

const fn tag(bytes: [u8; 4]) -> Tag {
    Tag::new(&bytes)
}

#[test]
fn shm_14_player_and_process_list_read_live_world_state() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(tag(*b"NPC_"), 7).zsub(tag(*b"EDID"), "Player"));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let game = Game::new(order);
    let player_id = GlobalFormId {
        file: 0,
        object: 0x14,
    };
    {
        let mut world = game.world_mut();
        let actual = world.player();
        assert_eq!(actual, player_id);
        world.reference(actual).unwrap().position = [10.0, 20.0, 30.0];
        world.actor(actual).unwrap();
    }

    let mut world = game.world_mut();
    {
        let mut player = PlayerCharacter::new(&mut *world).expect("player exists");
        assert_eq!(
            player.form_key(),
            Some(soe_commonlib_shim::world::FormKey {
                file: 0,
                object: 0x14
            })
        );
        assert_eq!(player.position(), Some([10.0, 20.0, 30.0]));
        assert!(player.actor_value("health").is_some());
    }
    let processes = ProcessLists::new(&*world);
    assert_eq!(
        processes.actor_forms(),
        vec![soe_commonlib_shim::world::FormKey {
            file: 0,
            object: 0x14
        }]
    );
    assert_eq!(
        world.actor_is_dead(soe_commonlib_shim::world::FormKey {
            file: 0,
            object: 0x14
        }),
        Some(false)
    );
}
