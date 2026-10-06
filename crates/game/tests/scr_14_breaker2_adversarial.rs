// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use crate::scr_13_actor_native_batch::{call, game, object};
use soe_game::world::ActorKeepOffset;
use soe_papyrus::Value;

#[test]
fn keep_offset_rejects_missing_required_coordinates_without_changing_request() {
    let mut game = game();
    let actor = game.form("GuardRef").unwrap();
    let target = game.form("CriminalRef").unwrap();
    let _ = game.world_mut().reference(actor);
    let _ = game.world_mut().reference(target);
    let original = ActorKeepOffset {
        target,
        offset: [1.0, 2.0, 3.0],
        angles: [0.0; 3],
        catch_up_radius: 20.0,
        follow_radius: 5.0,
    };
    assert!(game.world_mut().keep_actor_offset(actor, original));
    let target_value = object(&mut game, target);
    let actor_object = object(&mut game, actor);
    call(
        &mut game,
        actor_object,
        "KeepOffsetFromActor",
        &[target_value, Value::Float(9.0)],
    );
    assert_eq!(
        game.world_mut().actor(actor).unwrap().keep_offset,
        Some(original)
    );
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("expects target and three offset arguments")));
}
