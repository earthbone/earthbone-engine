// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{Compression, Header, RefId, Save, WString};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 0x800)
            .sub(Tag(*b"DATA"), [0, 0])
            .sub(Tag(*b"XOWN"), 0x810_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x811));
    plugin.push_top(Record::new(Tag(*b"LCTN"), 0x820));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order
}

fn save(owner: u32) -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("SAV-28"),
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
    let mut bytes = Vec::new();
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
    soe_save::GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.change_forms.push(soe_save::ChangeForm {
        form: reference(0x400800),
        change_flags: 1 << 3,
        type_field: 6,
        version: 78,
        uncompressed_len: 0,
        data: reference(0x400000 | owner).raw().to_be_bytes()[1..].to_vec(),
    });
    save
}

fn location_save(tail: &[u8]) -> Save {
    let mut saved = save(0x810);
    saved.change_forms = vec![soe_save::ChangeForm {
        form: reference(0x400820),
        change_flags: 0x8000_0001,
        type_field: 38,
        version: 78,
        uncompressed_len: 0,
        data: [
            0x12_34_56_78_u32.to_le_bytes().as_slice(),
            &[0x34, 0x12],
            tail,
        ]
        .concat(),
    }];
    saved
}

#[test]
fn sav_28_replacing_a_cell_owner_twice_keeps_the_latest_link_loadable() {
    let mut world = World::new(order());
    assert_eq!(world.cell_state(id(0x800)).unwrap().owner, Some(id(0x810)));
    world.restore_vanilla_spatial_records(&save(0x811)).unwrap();
    assert_eq!(world.cell_state(id(0x800)).unwrap().owner, Some(id(0x811)));
    world.restore_vanilla_spatial_records(&save(0x810)).unwrap();
    assert_eq!(world.cell_state(id(0x800)).unwrap().owner, Some(id(0x810)));

    let bytes = world.save_state();
    let mut loaded = World::new(order());
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.cell_state(id(0x800)).unwrap().owner, Some(id(0x810)));
}

#[test]
fn sav_28_location_placeholder_tail_changes_survive_engine_save() {
    let mut first = World::new(order());
    let mut second = World::new(order());
    first
        .restore_vanilla_spatial_records(&location_save(&[0xAA, 0xBB]))
        .unwrap();
    second
        .restore_vanilla_spatial_records(&location_save(&[0xCC, 0xDD]))
        .unwrap();
    assert_ne!(first.save_state(), second.save_state());
}
