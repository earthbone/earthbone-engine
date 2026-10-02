// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved deltas, leveled replacement and mapped per-stack identities.

use super::*;
use soe_game::inventory::{Field, Inventory};
use soe_save::ChangeForm;

fn change(holder: u32, delta: i32, stack: &[u8]) -> ChangeForm {
    let mut data = vec![4];
    reference(0x400800).write(&mut data);
    data.extend(delta.to_le_bytes());
    data.push(if stack.is_empty() { 0 } else { 4 });
    data.extend(stack);
    ChangeForm {
        form: reference(holder),
        type_field: 0,
        version: 0,
        change_flags: 1 << 5,
        data,
        uncompressed_len: 0,
    }
}

#[test]
fn saved_counts_are_deltas_and_stack_forms_follow_saved_plugin_owners() {
    let order = order();
    let mut save = save();
    save.form_id_array = vec![0x01000800];
    let mapping = SavedFormMap::new(&save, &order).unwrap();
    let mut fields = vec![16, 36];
    fields.extend(2_i16.to_le_bytes());
    fields.extend([22, 33]);
    reference(1).write(&mut fields);
    fields.push(40);
    fields.extend(0x7FC00042_u32.to_le_bytes());
    let parsed = change(0x400900, -3, &fields)
        .reference_contents(78)
        .unwrap()
        .unwrap()
        .inventory
        .unwrap();
    let mut authored = Inventory::default();
    authored.add(id(0, 0x800), 8).unwrap();
    let restored = authored.with_vanilla_changes(&parsed, &mapping).unwrap();
    assert_eq!(restored.count(id(0, 0x800)), 5);
    assert_eq!(authored.count(id(0, 0x800)), 8);
    let stack = &restored.stacks(id(0, 0x800))[0];
    assert_eq!(stack.count(), 2);
    assert!(stack.worn());
    assert_eq!(
        stack.extra(33).unwrap().fields,
        vec![Field::Form(Some(id(3, 0x800)))]
    );
    assert_eq!(
        stack.extra(40).unwrap().fields,
        vec![Field::FloatBits(0x7FC00042)]
    );
    assert!(stack.extra(36).is_none());
}

#[test]
fn leveled_entries_replace_authored_counts_and_rejected_deltas_leave_them_intact() {
    let order = order();
    let save = save();
    let mapping = SavedFormMap::new(&save, &order).unwrap();
    let parsed = change(0x400900, 1, &[4, 46, 0, 0, 0, 0, 1])
        .reference_contents(78)
        .unwrap()
        .unwrap()
        .inventory
        .unwrap();
    let mut authored = Inventory::default();
    authored.add(id(0, 0x800), 8).unwrap();
    let restored = authored.with_vanilla_changes(&parsed, &mapping).unwrap();
    assert_eq!(restored.count(id(0, 0x800)), 1);
    assert_eq!(restored.stacks(id(0, 0x800))[0].count(), 1);
    let invalid = change(0x400900, -9, &[])
        .reference_contents(78)
        .unwrap()
        .unwrap()
        .inventory
        .unwrap();
    assert!(authored.with_vanilla_changes(&invalid, &mapping).is_err());
    assert_eq!(authored.count(id(0, 0x800)), 8);
}

fn fixture_order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut cnto = 0x800_u32.to_le_bytes().to_vec();
    cnto.extend(10_i32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"CONT"), 0x801).sub(Tag(*b"CNTO"), cnto));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
    for object in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

#[test]
fn inventory_pass_validates_all_references_before_replacing_any_state() {
    let mut world = World::new(fixture_order());
    world.reference(id(0, 0x900)).unwrap();
    world.reference(id(0, 0x901)).unwrap();
    let mut save = save();
    save.plugins.truncate(1);
    save.light_plugins.clear();
    save.change_forms = vec![change(0x400900, -2, &[4, 22]), change(0x400901, -99, &[])];
    let before = world.state_hash();
    assert!(world.restore_vanilla_inventories(&save).is_err());
    assert_eq!(world.state_hash(), before);
    save.change_forms[1] = change(0x400901, 5, &[]);
    let report = world.restore_vanilla_inventories(&save).unwrap();
    assert_eq!(
        (report.references, report.entries, report.stacks),
        (2, 2, 1)
    );
    assert!(report.missing_items.is_empty());
    assert_eq!(world.item_count(id(0, 0x900), id(0, 0x800)), 8);
    assert_eq!(world.item_count(id(0, 0x901), id(0, 0x800)), 15);
    assert!(world.pending.is_empty());
    let expected = world.state_hash();
    world.restore_vanilla_inventories(&save).unwrap();
    assert_eq!(
        world.state_hash(),
        expected,
        "reconstruction uses authored counts, not current totals"
    );
    let mut loaded = World::new(fixture_order());
    loaded.load_state(&world.save_state()).unwrap();
    assert_eq!(loaded.state_hash(), expected);
}

#[test]
fn installed_non_actor_inventories_restore_distinct_stacks_and_persist() {
    installed_inventory_survey(false);
}

#[test]
fn installed_decodable_actor_inventories_restore_and_persist() {
    installed_inventory_survey(true);
}

fn installed_inventory_survey(actors: bool) {
    let (Some(_data), Some(saves)) = (
        std::env::var_os("SKYRIM_DATA"),
        std::env::var_os("SOE_TEST_SAVES"),
    ) else {
        eprintln!("SKYRIM_DATA or SOE_TEST_SAVES unset, skipping inventory restoration survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let order = Arc::new(order);
    let mut paths = vec![];
    collect_saves(Path::new(&saves), &mut paths);
    paths.sort();
    assert!(paths.len() >= 50);
    let (mut references, mut entries, mut stacks, mut missing) = (0, 0, 0, 0);
    let (mut unsupported, mut zero_worn) = (0, 0);
    for path in &paths {
        let mut save = Save::open(path).unwrap();
        let mut world = World::from_shared_order(Arc::clone(&order));
        world.restore_vanilla_npcs(&save).unwrap();
        world.restore_vanilla_reference_placements(&save).unwrap();
        if actors {
            world.restore_vanilla_actor_templates(&save).unwrap();
        }
        save.change_forms.retain(|form| {
            if form.type_index() != u8::from(actors)
                || form.change_flags & ((1 << 5) | (1 << 27)) == 0
            {
                return false;
            }
            if actors {
                if let Err(error) = form.reference_contents(save.form_version) {
                    assert!(
                        error
                            .to_string()
                            .contains("unsupported PackageData kind 18"),
                        "{error}"
                    );
                    unsupported += 1;
                    return false;
                }
            }
            true
        });
        let report = world
            .restore_vanilla_inventories(&save)
            .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert_eq!(report.references, save.change_forms.len());
        references += report.references;
        entries += report.entries;
        stacks += report.stacks;
        missing += report.missing_items.len();
        assert!(world.pending.is_empty());
        let mut loaded = World::from_shared_order(Arc::clone(&order));
        loaded.load_state(&world.save_state()).unwrap();
        assert_eq!(loaded.state_hash(), world.state_hash());
        let mapping = SavedFormMap::new(&save, &order).unwrap();
        for change in &save.change_forms {
            let id = mapping.reference(change.form).unwrap().unwrap();
            let inventory = &world.reference_ref(id).unwrap().inventory;
            zero_worn += inventory
                .iter()
                .filter(|(item, count)| {
                    *count == 0 && inventory.stacks(**item).iter().any(|stack| stack.worn())
                })
                .count();
            assert_eq!(loaded.reference_ref(id).unwrap().inventory, *inventory);
        }
    }
    assert!(references > 1000 && entries > 1000 && stacks > 1000);
    eprintln!(
        "{} saves, actors={actors}: {references} inventories, {entries} items, {stacks} distinct stacks, {missing} missing item definitions, {unsupported} unsupported contents, {zero_worn} zero-total worn items",
        paths.len()
    );
}
