// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::ObjectId;

const SCENE: u32 = 0x800;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn missing_parent_quest_scene() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"SCEN"), SCENE)
            .sub(Tag(*b"HNAM"), [])
            .zsub(Tag(*b"NAM0"), "RequiredPhase"),
    );
    let mut order = LoadOrder::new();
    order.add("Scr69MissingParent.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_69_forcestart_rejects_scene_without_required_parent_quest() {
    let mut game = missing_parent_quest_scene();
    let mut allocate = |_: &str, _: &str| ObjectId(1);

    assert!(
        !game.world_mut().force_start_scene(id(SCENE), &mut allocate),
        "TES5Edit marks SCEN PNAM Parent Quest as required"
    );
    assert!(!game.world().scenes.contains(&id(SCENE)));
}
