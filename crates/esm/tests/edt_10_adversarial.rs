#![allow(clippy::unwrap_used)]

use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::records::npc::{NpcFace, FACE_MORPH_COUNT};
use soe_esm::{LoadOrder, Tag};

#[test]
fn edt_10_duplicate_morph_blocks_are_not_silently_accepted_or_discarded() {
    let mut order = LoadOrder::new();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"HDPT"), 0x800));
    let mut npc = Record::new(Tag(*b"NPC_"), 0x801)
        .sub(Tag(*b"PNAM"), 0x800_u32.to_le_bytes())
        .sub(Tag(*b"NAM9"), [0; FACE_MORPH_COUNT * 4])
        .sub(Tag(*b"NAM9"), [1; FACE_MORPH_COUNT * 4]);
    npc.form_version = 44;
    plugin.push_top(npc.clone());
    order.add("Base.esm", plugin).unwrap();

    assert!(NpcFace::from_record(&npc).is_err());
}

#[test]
fn edt_10_repeated_head_part_links_keep_their_authored_order_on_edit() {
    let mut order = LoadOrder::new();
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"HDPT"), 0x800));
    base.push_top(Record::new(Tag(*b"HDPT"), 0x801));
    let npc = Record::new(Tag(*b"NPC_"), 0x802)
        .sub(Tag(*b"PNAM"), 0x801_u32.to_le_bytes())
        .sub(Tag(*b"PNAM"), 0x800_u32.to_le_bytes());
    base.push_top(npc.clone());
    order.add("Base.esm", base).unwrap();
    order
        .add(
            "Patch.esp",
            Plugin::new(FileHeader {
                masters: vec!["Base.esm".into()],
                ..Default::default()
            }),
        )
        .unwrap();

    let edited =
        NpcFace::edit_record(&npc, &[0x801, 0x800], [0.0; FACE_MORPH_COUNT], &order, 1).unwrap();
    let face = NpcFace::from_record(&edited).unwrap();
    assert_eq!(face.head_parts, [0x801, 0x800]);
}
