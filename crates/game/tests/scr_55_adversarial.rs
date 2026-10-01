// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

#[test]
fn setting_actor_base_flags_keeps_essential_and_protected_mutually_exclusive() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"NPC_"), 1));
    let mut order = LoadOrder::new();
    order
        .add("scr55-breaker.esp", plugin)
        .expect("plugin loads");
    let game = Game::new(order);
    let base = GlobalFormId { file: 0, object: 1 };
    game.world_mut().set_actor_base_essential(base, true);
    game.world_mut().set_actor_base_protected(base, true);
    let mut world = game.world_mut();
    let state = world.actor_base(base).expect("NPC_ state");
    assert!(
        !(state.essential && state.protected),
        "CK forbids both flags on one ActorBase"
    );
}
