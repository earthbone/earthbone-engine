// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record};
use soe_godot_host::facegen::{generate_npc_head, FaceGenError};

const NPC: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x900,
};

fn load_order(records: impl IntoIterator<Item = Record>) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order
        .add("Fixture.esp", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

#[test]
fn edt_22_missing_deleted_and_wrong_type_npcs_are_rejected() {
    let assets = Vfs::new();
    let missing = load_order([]);
    assert!(matches!(
        generate_npc_head(&missing, &assets, NPC),
        Err(FaceGenError::InvalidRecord { form: NPC, .. })
    ));

    let deleted =
        load_order([Record::new(b"NPC_", 0x900).with_flags(soe_esm::plugin::flags::DELETED)]);
    assert!(matches!(
        generate_npc_head(&deleted, &assets, NPC),
        Err(FaceGenError::InvalidRecord { form: NPC, .. })
    ));

    let wrong_type = load_order([Record::new(b"WEAP", 0x900)]);
    assert!(generate_npc_head(&wrong_type, &assets, NPC).is_err());
}

#[test]
fn edt_22_pnam_links_must_resolve_to_live_head_parts() {
    let assets = Vfs::new();
    let order = load_order([
        Record::new(b"NPC_", 0x900).sub(b"PNAM", 0x800_u32.to_le_bytes()),
        Record::new(b"WEAP", 0x800),
    ]);

    assert!(matches!(
        generate_npc_head(&order, &assets, NPC),
        Err(FaceGenError::InvalidRecord { .. })
    ));
}
