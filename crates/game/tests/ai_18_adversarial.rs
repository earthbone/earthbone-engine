// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::{Game, quest};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn ai_18_required_location_alias_rejects_a_deleted_winning_lctn() {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag(*b"LCTN"), 0x801));
    plugin.push_top(Record::new(Tag(*b"LCTN"), 0x802).with_flags(flags::DELETED));
    plugin.push_top(
        quest::build_record(0x800, "DeletedLocation", 0)
            .sub(Tag(*b"ALLS"), 3u32.to_le_bytes())
            .zsub(Tag(*b"ALID"), "Deleted")
            .sub(Tag(*b"FNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"ALFL"), 0x802u32.to_le_bytes())
            .sub(Tag(*b"ALED"), Vec::new()),
    );
    let mut order = LoadOrder::new();
    order.add("DeletedLocation.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(!game.start_quest(id(0x800)));
    assert!(!game.world_mut().quest(id(0x800)).unwrap().running);
    assert_eq!(game.world_mut().alias_ref(id(0x800), 3), None);
}
