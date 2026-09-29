// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{Compression, Header, RefId, Save, WString};

const FACT: u32 = 0x801;
const CLASS: u32 = 0x800;
const PARENT: u32 = 0x802;
const CHILD: u32 = 0x803;
const RELA: u32 = 0x804;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Prisoner"),
        player_level: 1,
        player_location: WString::default(),
        game_date: WString::default(),
        player_race: WString::default(),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 0.0,
        filetime: 0,
        screenshot_width: 0,
        screenshot_height: 0,
        compression: Compression::None,
        screenshot: vec![],
    };
    let mut bytes = vec![];
    header.write(&mut bytes);
    let mut plugins = vec![1];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let at = bytes.len() as u32 + 100;
    soe_save::FileLocationTable {
        form_id_array_count_offset: at + 8,
        unknown_table_3_offset: at + 16,
        table_1_offset: at,
        table_2_offset: at,
        change_forms_offset: at,
        table_3_offset: at,
        table_1_count: 0,
        table_2_count: 0,
        table_3_count: 0,
        change_form_count: 0,
        unused: [0; 15],
    }
    .write(&mut bytes);
    soe_save::GlobalData { kind: 1000, data: vec![] }.write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"CLAS"), CLASS).sub(Tag(*b"DATA"), [0; 16]));
    plugin.push_top(
        Record::new(Tag(*b"FACT"), FACT)
            .sub(Tag(*b"DATA"), 0_u32.to_le_bytes())
            .sub(Tag(*b"XNAM"), [0x02, 0x08, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]),
    );
    plugin.push_top(Record::new(Tag(*b"NPC_"), PARENT));
    plugin.push_top(Record::new(Tag(*b"NPC_"), CHILD));
    let mut data = vec![0; 16];
    data[0..4].copy_from_slice(&0x802_u32.to_le_bytes());
    data[4..8].copy_from_slice(&0x803_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"RELA"), RELA).sub(Tag(*b"DATA"), data));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn change(object: u32, kind: u8, flags: u32, data: Vec<u8>) -> soe_save::ChangeForm {
    soe_save::ChangeForm {
        form: reference(0x400000 | object),
        change_flags: flags,
        type_field: kind,
        version: 78,
        uncompressed_len: 0,
        data,
    }
}

#[test]
fn sav_46_relationship_rank_enum_end_survives_engine_save_load() {
    let order = Arc::new(order());
    let mut saved = save();
    // RELA's saved rank is a four byte value. Plugin rank 8 is Archnemesis,
    // which the actor-facing rank scale represents as -4.
    saved.change_forms.push(change(RELA, 39, 2, 8_i32.to_le_bytes().to_vec()));
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.restore_vanilla_record_changes(&saved).unwrap(), 1);
    assert_eq!(world.relationship_record_rank(id(0, PARENT), id(0, CHILD)), Some(-4));

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.relationship_record_rank(id(0, PARENT), id(0, CHILD)), Some(-4));
}

#[test]
fn sav_46_present_empty_faction_reactions_clear_warm_record_consumer() {
    let order = Arc::new(order());
    let mut world = World::from_shared_order(order.clone());
    assert_eq!(world.faction(id(0, FACT)).unwrap().reactions.len(), 1);
    let mut saved = save();
    // A set reactions bit with a zero count means an empty replacement list.
    saved.change_forms.push(change(FACT, 31, 4, vec![0]));
    assert_eq!(world.restore_vanilla_record_changes(&saved).unwrap(), 1);
    assert!(world.faction(id(0, FACT)).unwrap().reactions.is_empty());
    assert!(world.record(id(0, FACT)).unwrap().all(Tag(*b"XNAM")).next().is_none());

    let bytes = world.save_state();
    let mut loaded = World::from_shared_order(order);
    loaded.load_state(&bytes).unwrap();
    assert!(loaded.faction(id(0, FACT)).unwrap().reactions.is_empty());
    assert!(loaded.record(id(0, FACT)).unwrap().all(Tag(*b"XNAM")).next().is_none());
}

#[test]
fn sav_46_unresolved_faction_reaction_does_not_apply_prior_record_changes() {
    let order = Arc::new(order());
    let mut saved = save();
    // The first CLAS change is valid, while this reaction names a slot absent
    // from the saved plugin lists. Neither change may reach the live world.
    saved.change_forms.push(change(CLASS, 30, 0, vec![]));
    let mut fact = vec![1];
    reference(0x01000800).write(&mut fact);
    fact.extend(0_i32.to_le_bytes());
    fact.extend(1_u32.to_le_bytes());
    saved.change_forms.push(change(FACT, 31, 4, fact));
    let mut world = World::from_shared_order(order);
    let before = world.save_state();
    assert!(world.restore_vanilla_record_changes(&saved).is_err());
    assert_eq!(world.save_state(), before);
    assert!(world.record(id(0, FACT)).unwrap().all(Tag(*b"XNAM")).next().is_some());
}
