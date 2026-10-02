#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{GlobalFormId, LoadOrder, Plugin, Record, Tag, plugin::FileHeader};
use soe_game::{World, world::DYNAMIC_FILE};
use soe_save::{
    ChangeForm, Compression, GlobalData, Header, RefId, Save, WString, reference::PlayerLocation,
};

fn rid(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn fixture() -> (World, Save) {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    master.push_top(Record::new(Tag(*b"CELL"), 0x800));
    master.push_top(Record::new(Tag(*b"MGEF"), 0x900));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", master).unwrap();
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
    save.table_1.push(GlobalData {
        kind: 1,
        data: PlayerLocation {
            next_object_id: 0xFF000900,
            world: rid(0),
            coordinates: [0; 2],
            cell_or_world: rid(0x400800),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    });
    let mut player = rid(0x400800).raw().to_be_bytes()[1..].to_vec();
    player.extend(
        [1.0_f32, 2.0, 3.0, 0.0, 0.5, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    player.extend_from_slice(&[0; 8]);
    save.change_forms.push(ChangeForm {
        form: rid(0x400014),
        change_flags: 1 << 1,
        type_field: 1,
        version: 78,
        uncompressed_len: 0,
        data: player,
    });
    let mut objects = vec![4, 0x80, 0x09, 0];
    objects.extend_from_slice(&7_u32.to_le_bytes());
    objects.push(4);
    rid(0x400900).write(&mut objects);
    objects.extend_from_slice(&12.5_f32.to_le_bytes());
    objects.extend_from_slice(&30_u32.to_le_bytes());
    objects.extend_from_slice(&2_u32.to_le_bytes());
    objects.extend_from_slice(&4.25_f32.to_le_bytes());
    objects.extend_from_slice(&[0, 0, 0]);
    save.table_1.push(GlobalData {
        kind: GlobalData::CREATED_OBJECTS,
        data: objects,
    });
    (World::new(order), save)
}

#[test]
fn sav_17_created_form_identity_and_effect_link_survive_repeated_engine_saves() {
    let (mut world, save) = fixture();
    world.restore_vanilla_reference_placements(&save).unwrap();
    let base = GlobalFormId {
        file: DYNAMIC_FILE,
        object: 0x900,
    };
    let first = world.save_state();
    let mut loaded = {
        let (fresh, _) = fixture();
        fresh
    };
    loaded.load_state(&first).unwrap();
    let second = loaded.save_state();
    let mut reloaded = {
        let (fresh, _) = fixture();
        fresh
    };
    reloaded.load_state(&second).unwrap();

    let record = reloaded.record(base).unwrap();
    assert_eq!(record.tag, Tag(*b"ENCH"));
    let raw = u32::from_le_bytes(record.find(Tag(*b"EFID")).unwrap().try_into().unwrap());
    assert_eq!(
        reloaded.resolve_from(base, raw),
        Some(GlobalFormId {
            file: 0,
            object: 0x900
        })
    );
}
