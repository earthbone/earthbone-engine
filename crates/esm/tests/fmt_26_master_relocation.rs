#![allow(clippy::unwrap_used)]

use soe_esm::load_order::{GlobalFormId, LoadOrder};
use soe_esm::plugin::{FileHeader, Group, GroupKind, Plugin, Record};
use soe_esm::tag::{self, Tag};

fn master(name: &str, form_id: u32) -> (String, Plugin) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"GLOB"), form_id));
    (name.to_string(), plugin)
}

fn target() -> Plugin {
    let header = FileHeader {
        masters: vec!["A.esm".into(), "B.esm".into()],
        ..FileHeader::default()
    };
    let mut plugin = Plugin::new(header);
    plugin.push_top(Record::new(tag::REFR, 0x0000_0801).sub(b"NAME", 0x0000_0802u32.to_le_bytes()));
    plugin.push_top(Record::new(tag::REFR, 0x0100_0802).sub(b"NAME", 0x0200_0803u32.to_le_bytes()));
    plugin.push_top(Record::new(tag::REFR, 0x0200_0803).sub(b"NAME", 0x0000_0801u32.to_le_bytes()));
    let children = Group::new(GroupKind::CellChildren(0x0200_0803));
    plugin.groups.push(children);
    plugin
}

fn identities(order: &LoadOrder, file: usize) -> Vec<(String, u32)> {
    order
        .file(file)
        .plugin
        .records
        .iter()
        .map(|record| {
            let id: GlobalFormId = order.resolve(file, record.form_id);
            (order.file(id.file as usize).name.clone(), id.object)
        })
        .collect()
}

#[test]
fn fmt_26_reordering_and_adding_masters_preserves_record_and_link_identities() {
    let (a_name, a) = master("A.esm", 0x800);
    let (b_name, b) = master("B.esm", 0x800);
    let (c_name, c) = master("C.esm", 0x800);
    let mut before = LoadOrder::new();
    before.add(&a_name, a.clone()).unwrap();
    before.add(&b_name, b.clone()).unwrap();
    let target_index = before.add("Patch.esp", target()).unwrap();
    let prior_identities = identities(&before, target_index);

    let mut changed = target();
    changed
        .set_masters(vec!["B.esm".into(), "A.esm".into(), "C.esm".into()])
        .unwrap();
    let bytes = changed.to_bytes();
    let reloaded = Plugin::parse(&bytes).unwrap();
    let mut after = LoadOrder::new();
    after.add(&a_name, a).unwrap();
    after.add(&b_name, b).unwrap();
    after.add(&c_name, c).unwrap();
    let target_index = after.add("Patch.esp", reloaded).unwrap();

    assert_eq!(identities(&after, target_index), prior_identities);
    let patch = &after.file(target_index).plugin;
    assert_eq!(patch.header.masters, ["B.esm", "A.esm", "C.esm"]);
    assert_eq!(
        patch.records[0].find(Tag(*b"NAME")),
        Some(&0x0100_0802u32.to_le_bytes()[..])
    );
    assert_eq!(
        patch.records[1].find(Tag(*b"NAME")),
        Some(&0x0300_0803u32.to_le_bytes()[..])
    );
    assert_eq!(
        patch.records[2].find(Tag(*b"NAME")),
        Some(&0x0100_0801u32.to_le_bytes()[..])
    );
    assert!(patch
        .groups
        .iter()
        .any(|group| { group.kind == GroupKind::CellChildren(0x0300_0803) }));
}

#[test]
fn fmt_26_rejects_duplicate_or_removed_masters_without_mutation() {
    let mut plugin = target();
    let before = plugin.clone();
    assert!(plugin
        .set_masters(vec!["A.esm".into(), "a.ESM".into(), "B.esm".into()])
        .is_err());
    assert_eq!(plugin, before);
    assert!(plugin.set_masters(vec!["A.esm".into()]).is_err());
    assert_eq!(plugin, before);
}
