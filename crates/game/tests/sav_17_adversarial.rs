// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::FileHeader};
use soe_game::{World, world::DYNAMIC_FILE};
use soe_save::{
    ChangeForm, Compression, GlobalData, Header, RefId, Save, WString, reference::PlayerLocation,
};

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn ref_id(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn order() -> LoadOrder {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    master.push_top(Record::new(Tag(*b"CELL"), 0x800));
    master.push_top(Record::new(Tag(*b"MGEF"), 0x900));
    let mut result = LoadOrder::new();
    result.add("Skyrim.esm", master).unwrap();
    let mut light = Plugin::new(FileHeader {
        flags: soe_esm::plugin::flags::LIGHT_MASTER,
        ..Default::default()
    });
    light.push_top(Record::new(Tag(*b"MGEF"), 0x901));
    result.add("Extra.esl", light).unwrap();
    result
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
    GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.light_plugins = vec![WString::from_text("Extra.esl")];
    save.table_1.push(GlobalData {
        kind: 1,
        data: PlayerLocation {
            next_object_id: 0xFF000A00,
            world: ref_id(0),
            coordinates: [0; 2],
            cell_or_world: ref_id(0x400800),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    });
    save.change_forms.push(ChangeForm {
        form: ref_id(0x400014),
        change_flags: 1 << 1,
        type_field: 1,
        version: 0,
        uncompressed_len: 0,
        data: [
            ref_id(0x400800).raw().to_be_bytes()[1..].to_vec(),
            [1.0_f32, 2.0, 3.0, 0.0, 0.5, 0.0]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect(),
            vec![0; 8],
        ]
        .concat(),
    });
    save
}

fn created_object(out: &mut Vec<u8>, object: u32, effect: Option<(u32, f32, u32, u32, f32)>) {
    ref_id(0x800000 | object).write(out);
    out.extend_from_slice(&7_u32.to_le_bytes());
    if let Some((form, magnitude, duration, area, price)) = effect {
        out.push(4);
        ref_id(form).write(out);
        out.extend_from_slice(&magnitude.to_le_bytes());
        out.extend_from_slice(&duration.to_le_bytes());
        out.extend_from_slice(&area.to_le_bytes());
        out.extend_from_slice(&price.to_le_bytes());
    } else {
        out.push(0);
    }
}

type CreatedEffect = (u32, f32, u32, u32, f32);
type CreatedEntry = Option<(u32, u32, Option<CreatedEffect>)>;

fn created_tables(entries: [CreatedEntry; 4]) -> Vec<u8> {
    let mut out = vec![];
    for entry in entries {
        if let Some((object, _times_used, effect)) = entry {
            out.push(4);
            created_object(&mut out, object, effect);
        } else {
            out.push(0);
        }
    }
    out
}

fn with_created(save: &mut Save, data: Vec<u8>) {
    save.table_1.push(GlobalData {
        kind: GlobalData::CREATED_OBJECTS,
        data,
    });
}

#[test]
fn sav_17_all_created_tables_reserve_forms_and_round_trip_effect_links() {
    let mut save = save();
    save.form_id_array.push(0xFE000901);
    with_created(
        &mut save,
        created_tables([
            Some((0x900, 7, Some((0x400900, 12.5, 30, 2, 4.25)))),
            Some((0x901, 7, None)),
            Some((0x902, 7, Some((0x000001, 8.0, 5, 3, 1.0)))),
            Some((0x903, 7, None)),
        ]),
    );
    let mut world = World::new(order());
    world.restore_vanilla_reference_placements(&save).unwrap();
    for (object, tag) in [
        (0x900, Tag(*b"ENCH")),
        (0x901, Tag(*b"ENCH")),
        (0x902, Tag(*b"ALCH")),
        (0x903, Tag(*b"ALCH")),
    ] {
        assert_eq!(world.record(id(DYNAMIC_FILE, object)).unwrap().tag, tag);
    }
    assert_eq!(
        world.place_at(id(0, 0x900), id(0, 0x14), false),
        id(DYNAMIC_FILE, 0xA00)
    );
    let bytes = world.save_state();
    let mut loaded = World::new(order());
    loaded.load_state(&bytes).unwrap();
    let record = loaded.record(id(DYNAMIC_FILE, 0x900)).unwrap();
    assert_eq!(
        record.find(Tag(*b"EFIT")),
        Some(&[0, 0, 72, 65, 2, 0, 0, 0, 30, 0, 0, 0][..])
    );
    let raw = u32::from_le_bytes(record.find(Tag(*b"EFID")).unwrap().try_into().unwrap());
    assert_eq!(
        loaded.resolve_from(id(DYNAMIC_FILE, 0x900), raw),
        Some(id(0, 0x900))
    );
    let potion = loaded.record(id(DYNAMIC_FILE, 0x902)).unwrap();
    let raw = u32::from_le_bytes(potion.find(Tag(*b"EFID")).unwrap().try_into().unwrap());
    assert_eq!(
        loaded.resolve_from(id(DYNAMIC_FILE, 0x902), raw),
        Some(id(1, 0x901))
    );
}

#[test]
fn sav_17_unmapped_effect_rejects_without_partial_world_mutation() {
    let mut save = save();
    with_created(
        &mut save,
        created_tables([
            Some((0x900, 1, Some((0x01000900, 1.0, 0, 0, 0.0)))),
            None,
            None,
            None,
        ]),
    );
    let mut world = World::new(order());
    let before = world.state_hash();
    assert!(world.restore_vanilla_reference_placements(&save).is_err());
    assert_eq!(world.state_hash(), before);
}

#[test]
fn sav_17_duplicate_created_identity_rejects_without_partial_world_mutation() {
    let mut save = save();
    with_created(
        &mut save,
        created_tables([Some((0x900, 1, None)), Some((0x900, 1, None)), None, None]),
    );
    let mut world = World::new(order());
    let before = world.state_hash();
    assert!(world.restore_vanilla_reference_placements(&save).is_err());
    assert_eq!(world.state_hash(), before);
}
