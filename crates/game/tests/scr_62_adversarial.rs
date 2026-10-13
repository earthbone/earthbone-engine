// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

#[test]
fn scr_62_global_variable_and_owner_bindings_are_methods() {
    let game = Game::new(LoadOrder::new());
    for class in ["Alias", "Scene", "Package", "TopicInfo"] {
        assert!(
            game.vm.natives.has_method(class, "GetOwningQuest"),
            "{class}.GetOwningQuest"
        );
    }
    for name in ["GetValue", "GetValueInt", "SetValue", "SetValueInt", "Mod"] {
        assert!(
            game.vm.natives.has_method("GlobalVariable", name),
            "GlobalVariable.{name}"
        );
    }
    assert!(!game.vm.natives.has_global("GlobalVariable", "Mod"));
}

#[test]
fn scr_62_scene_owner_uses_parent_quest_after_action_packages() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"SCEN"), 0x800)
            .sub(Tag(*b"ANAM"), 1u16.to_le_bytes())
            .sub(Tag(*b"PNAM"), 0x811u32.to_le_bytes())
            .sub(Tag(*b"PNAM"), 0x812u32.to_le_bytes())
            .sub(Tag(*b"ANAM"), [])
            .sub(Tag(*b"PNAM"), 0x810u32.to_le_bytes()),
    );
    plugin.push_top(soe_game::quest::build_record(0x810, "ParentQuest", 0));
    let mut order = LoadOrder::new();
    order.add("Scene.esp", plugin).unwrap();
    let game = Game::new(order);
    assert_eq!(
        game.world().owning_quest(GlobalFormId {
            file: 0,
            object: 0x800
        }),
        Some(GlobalFormId {
            file: 0,
            object: 0x810
        })
    );
}
