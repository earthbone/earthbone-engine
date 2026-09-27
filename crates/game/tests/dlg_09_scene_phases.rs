// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-09: SCEN phase progression waits for ending actions and survives SOEW.
//! SCEN fields follow local xEdit and UESP definitions. The CK scene pages are
//! absent from the local corpus, so action completion is supplied by the
//! owning world API in these runtime tests.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::ObjectId;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn scene() -> Record {
    Record::new(Tag(*b"SCEN"), 0x800)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Second")
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Wait")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 10u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Second action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 11u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), [])
}

fn fixture_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(scene());
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    Game::new(order)
}

/// dlg_09_scene_waits_for_required_actions_and_persists_its_phase
#[test]
fn dlg_09_scene_waits_for_required_actions_and_persists_its_phase() {
    let game = fixture_game();
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    let mut world = game.world_mut();
    assert!(world.start_scene(id(0x800), &mut allocate));
    assert_eq!(world.scene_phase(id(0x800)), Some(0));
    assert!(!world.advance_scene_phase(id(0x800), &mut allocate));
    assert!(!world.complete_scene_action(id(0x800), 2));
    assert!(world.complete_scene_action(id(0x800), 0));
    assert!(!world.advance_scene_phase(id(0x800), &mut allocate));
    assert!(world.complete_scene_action(id(0x800), 1));
    assert!(world.advance_scene_phase(id(0x800), &mut allocate));
    assert_eq!(world.scene_phase(id(0x800)), Some(1));
    drop(world);

    let save = game.save_state().unwrap();
    let mut restored = fixture_game();
    restored.load_state(&save).unwrap();
    assert_eq!(restored.world().scene_phase(id(0x800)), Some(1));
    assert!(restored.world().scenes.contains(&id(0x800)));
}
