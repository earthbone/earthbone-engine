// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::ObjectId;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let scene = Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Second")
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Action A")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 1u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Action B")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 2u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), []);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(scene);
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn dlg_09_completed_action_set_survives_save_and_duplicate_completion_is_ignored() {
    let game = fixture();
    let mut alloc = |_: &str, _: &str| ObjectId(1);
    assert!(game.world_mut().start_scene(id(0x800), &mut alloc));
    assert!(game.world_mut().complete_scene_action(id(0x800), 0));
    let save = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&save).unwrap();
    assert!(!restored.world_mut().complete_scene_action(id(0x800), 0));
    assert!(!restored
        .world_mut()
        .advance_scene_phase(id(0x800), &mut alloc));
    assert!(restored.world_mut().complete_scene_action(id(0x800), 1));
    assert!(restored
        .world_mut()
        .advance_scene_phase(id(0x800), &mut alloc));
    assert_eq!(restored.world().scene_phase(id(0x800)), Some(1));
}
