#![allow(clippy::unwrap_used)]

use soe_esm::load_order::LoadOrder;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::records::npc::{NpcFace, FACE_MORPH_COUNT};
use soe_esm::tag::Tag;

const NPC: Tag = Tag(*b"NPC_");
const HDPT: Tag = Tag(*b"HDPT");

fn master() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(HDPT, 0x800));
    plugin.push_top(Record::new(HDPT, 0x801));
    plugin.push_top(Record::new(b"WEAP", 0x802));
    let morph: [f32; FACE_MORPH_COUNT] = std::array::from_fn(|index| index as f32);
    let mut npc = Record::new(NPC, 0x803)
        .sub(b"PNAM", 0x800u32.to_le_bytes())
        .sub(b"PNAM", 0x801u32.to_le_bytes())
        .sub(b"NAM9", morph_bytes(&morph))
        .sub(b"NAMA", [4; 16])
        .sub(b"NAM7", 0.75f32.to_le_bytes());
    npc.form_version = 44;
    plugin.push_top(npc);
    plugin
}

fn morph_bytes(morph: &[f32; FACE_MORPH_COUNT]) -> Vec<u8> {
    morph.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn fixture_order() -> LoadOrder {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::parse(&master().to_bytes()).unwrap())
        .unwrap();
    order
}

#[test]
fn edt_10_npc_face_edit_saves_validated_override_and_reloads_same_links() {
    let mut order = fixture_order();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    order.add("FacePatch.esp", patch.clone()).unwrap();
    let base_npc = order.file(0).plugin.records_of(NPC).next().unwrap();
    let edited_morph = std::array::from_fn(|index| index as f32 * 0.125 - 0.5);
    let edited = NpcFace::edit_record(base_npc, &[0x800, 0x801], edited_morph, &order, 1).unwrap();
    patch.push_top(edited);

    let saved = Plugin::parse(&patch.to_bytes()).unwrap();
    let saved_face = NpcFace::from_record(saved.records_of(NPC).next().unwrap()).unwrap();
    assert_eq!(saved_face.head_parts, [0x800, 0x801]);
    assert_eq!(saved_face.morph, edited_morph);
    assert_eq!(
        saved.records_of(NPC).next().unwrap().find(Tag(*b"NAMA")),
        Some(&[4; 16][..])
    );
    assert_eq!(
        saved.records_of(NPC).next().unwrap().find(Tag(*b"NAM7")),
        Some(&0.75f32.to_le_bytes()[..])
    );

    let mut reloaded_order = fixture_order();
    reloaded_order.add("FacePatch.esp", saved).unwrap();
    let winner = reloaded_order
        .winner(soe_esm::GlobalFormId {
            file: 0,
            object: 0x803,
        })
        .unwrap();
    let reloaded_face = NpcFace::from_record(winner).unwrap();
    assert_eq!(reloaded_face.head_parts, [0x800, 0x801]);
    assert_eq!(
        reloaded_face
            .head_parts
            .iter()
            .map(|raw| reloaded_order.resolve(1, *raw))
            .collect::<Vec<_>>(),
        [
            soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            },
            soe_esm::GlobalFormId {
                file: 0,
                object: 0x801
            },
        ]
    );
    assert_eq!(reloaded_face.morph, edited_morph);
}

#[test]
fn edt_10_npc_face_edit_rejects_missing_or_wrong_type_head_parts() {
    let order = fixture_order();
    let npc = order.file(0).plugin.records_of(NPC).next().unwrap();
    let morph = [0.0; FACE_MORPH_COUNT];
    assert!(NpcFace::edit_record(npc, &[0x999], morph, &order, 0).is_err());
    assert!(NpcFace::edit_record(npc, &[0x802], morph, &order, 0).is_err());
}

#[test]
fn edt_10_npc_face_edit_rejects_non_finite_morph_values() {
    let order = fixture_order();
    let npc = order.file(0).plugin.records_of(NPC).next().unwrap();
    let mut morph = [0.0; FACE_MORPH_COUNT];
    morph[7] = f32::INFINITY;
    assert!(NpcFace::edit_record(npc, &[0x800], morph, &order, 0).is_err());
}

#[test]
fn edt_10_npc_face_view_rejects_malformed_head_part_link() {
    let npc = Record::new(NPC, 0x803).sub(b"PNAM", [1, 2, 3]);
    assert!(NpcFace::from_record(&npc).is_err());
}

#[test]
fn edt_10_npc_head_part_links_relocate_when_master_order_changes() {
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(
        Record::new(NPC, 0x803)
            .sub(b"PNAM", 0x800u32.to_le_bytes())
            .sub(b"PNAM", 0x801u32.to_le_bytes()),
    );
    patch
        .set_masters(vec!["Other.esm".into(), "Skyrim.esm".into()])
        .unwrap();
    let reloaded = Plugin::parse(&patch.to_bytes()).unwrap();
    let links = reloaded.records_of(NPC).next().unwrap();
    assert_eq!(
        links.all(Tag(*b"PNAM")).collect::<Vec<_>>(),
        [
            &0x0100_0800u32.to_le_bytes()[..],
            &0x0100_0801u32.to_le_bytes()[..],
        ]
    );
}
