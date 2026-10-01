// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::LoadOrder;
use soe_game::world::World;

#[test]
fn scr_36_help_event_suppresses_duplicates_until_reset_then_shows_again() {
    let mut world = World::new(LoadOrder::new());
    world.show_help_message("Activate", "Use the door".into());
    world.show_help_message("Activate", "Use the door".into());
    assert_eq!(world.notifications.len(), 1);
    world.reset_help_message("Activate");
    world.show_help_message("Activate", "Use the door".into());
    assert_eq!(world.notifications.len(), 2);
}
