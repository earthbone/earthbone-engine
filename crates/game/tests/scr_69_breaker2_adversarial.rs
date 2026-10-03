// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::ObjectId;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn scr_69_force_start_rejects_scene_missing_required_parent_quest() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"SCEN"), 0x800)
            .sub(Tag(*b"HNAM"), [])
            .zsub(Tag(*b"NAM0"), "Phase"),
    );
    let mut order = LoadOrder::new();
    order.add("Scr69MissingQuest.esp", plugin).unwrap();
    let game = Game::new(order);
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(
        !game.world_mut().force_start_scene(id(0x800), &mut allocate),
        "TES5Edit declares SCEN PNAM Parent Quest required"
    );
    assert!(!game.world().scenes.contains(&id(0x800)));
}
