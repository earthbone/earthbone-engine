#![allow(clippy::unwrap_used, clippy::expect_used)]

use soe_esm::{plugin, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::World;
use soe_save::{
    reference::{flags, PlayerLocation},
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
};

fn ref_id(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn save(mask: u32) -> Save {
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
    FileLocationTable {
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
            world: ref_id(0),
            coordinates: [0; 2],
            cell_or_world: ref_id(0x400800),
            position: [1.0, 2.0, 3.0],
            unknown: 0,
        }
        .data_bytes(),
    });
    let mut player = ref_id(0x400800).raw().to_be_bytes()[1..].to_vec();
    player.extend(
        [1.0_f32, 2.0, 3.0, 0.0, 0.5, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes),
    );
    player.extend_from_slice(&[0; 20]);
    save.change_forms.push(ChangeForm {
        form: ref_id(0x400015),
        change_flags: flags::MOVE | flags::FORM,
        type_field: 0,
        version: 78,
        uncompressed_len: 0,
        data: {
            let mut data = ref_id(0x400800).raw().to_be_bytes()[1..].to_vec();
            data.extend(
                [1.0_f32, 2.0, 3.0, 0.0, 0.5, 0.0]
                    .into_iter()
                    .flat_map(f32::to_le_bytes),
            );
            data.extend_from_slice(&mask.to_le_bytes());
            data.extend_from_slice(&0_u16.to_le_bytes());
            data
        },
    });
    save.change_forms.push(ChangeForm {
        form: ref_id(0x400014),
        change_flags: flags::MOVE,
        type_field: 1,
        version: 78,
        uncompressed_len: 0,
        data: {
            let mut data = player;
            data.extend_from_slice(&[0; 8]);
            data
        },
    });
    save
}

fn world() -> World {
    let mut plugin = Plugin::new(soe_esm::plugin::FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"EDID"), b"Player\0".to_vec()));
    plugin.push_top(Record::new(Tag(*b"CELL"), 0x800));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x900));
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 0x15)
            .with_flags(plugin::flags::INITIALLY_DISABLED)
            .sub(Tag(*b"NAME"), 0x00000900_u32.to_le_bytes().to_vec()),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    World::new(order)
}

#[test]
fn saved_inversion_mask_xors_authored_disabled_and_persistent_flags() {
    let mut world = world();
    world
        .restore_vanilla_reference_placements(&save(
            plugin::flags::INITIALLY_DISABLED | plugin::flags::PERSISTENT,
        ))
        .unwrap();
    let state = world
        .reference_ref(GlobalFormId {
            file: 0,
            object: 0x15,
        })
        .unwrap();
    assert!(
        state.enabled,
        "the disabled bit is cleared by the saved inversion mask"
    );
    assert!(
        state.persistent,
        "the persistent bit is set by the saved inversion mask"
    );
}

#[test]
fn saved_mask_does_not_change_authored_bits_outside_disabled_and_persistent() {
    let mut world = world();
    world
        .restore_vanilla_reference_placements(&save(plugin::flags::VISIBLE_WHEN_DISTANT))
        .unwrap();
    let state = world
        .reference_ref(GlobalFormId {
            file: 0,
            object: 0x15,
        })
        .unwrap();
    assert!(
        !state.enabled,
        "an unrelated mask bit preserves the authored disabled state"
    );
    assert!(!state.persistent);
}

#[test]
fn unsupported_mask_version_rejects_before_reference_state_changes() {
    let mut save = save(plugin::flags::INITIALLY_DISABLED);
    save.form_version = 79;
    let mut world = world();
    let before = world.state_hash();

    assert!(world.restore_vanilla_reference_placements(&save).is_err());
    assert_eq!(world.state_hash(), before);
}
