// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::{vanilla_save::RestoreError, Game};
use soe_papyrus::asm::ScriptBuilder;
use soe_save::{Compression, GlobalData, Header, Save, WString};

fn save(raw_form: u32, unknown: u16, refid: [u8; 3]) -> Save {
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
    let mut bytes = Vec::new();
    header.write(&mut bytes);
    let mut plugins = vec![1];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let offset = bytes.len() as u32 + 100;
    soe_save::FileLocationTable {
        form_id_array_count_offset: offset + 8,
        unknown_table_3_offset: offset + 16,
        table_1_offset: offset,
        table_2_offset: offset,
        change_forms_offset: offset,
        table_3_offset: offset,
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
    let mut save = Save::parse(&bytes).expect("fixture parses");
    save.plugins = vec![WString::from_text("Skyrim.esm")];
    save.form_id_array = vec![raw_form];

    let mut papyrus = Vec::new();
    papyrus.extend_from_slice(&2_u16.to_le_bytes());
    papyrus.extend_from_slice(&3_u32.to_le_bytes());
    for value in ["BoundScript", "", "Saved"] {
        WString::from_text(value).write(&mut papyrus);
    }
    papyrus.extend_from_slice(&1_u32.to_le_bytes());
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    papyrus.extend_from_slice(&1_u32.to_le_bytes());
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    papyrus.extend_from_slice(&1_u32.to_le_bytes());
    papyrus.extend_from_slice(&41_u64.to_le_bytes());
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    papyrus.extend_from_slice(&0_u16.to_le_bytes());
    papyrus.extend_from_slice(&unknown.to_le_bytes());
    papyrus.extend_from_slice(&refid);
    papyrus.push(0);
    for _ in 0..2 {
        papyrus.extend_from_slice(&0_u32.to_le_bytes());
    }
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    papyrus.extend_from_slice(&41_u64.to_le_bytes());
    papyrus.push(0);
    papyrus.extend_from_slice(&2_u32.to_le_bytes());
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    papyrus.extend_from_slice(&0_u32.to_le_bytes());
    for _ in 0..6 {
        papyrus.extend_from_slice(&0_u32.to_le_bytes());
    }
    papyrus.extend_from_slice(&0_u16.to_le_bytes());
    save.table_3 = vec![GlobalData {
        kind: GlobalData::PAPYRUS,
        data: papyrus,
    }];
    save
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x800));
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.load_script(
        &ScriptBuilder::new("BoundScript", "")
            .auto_state("Saved")
            .bytes(),
    )
    .unwrap();
    game
}

#[test]
fn saved_refid_resolves_form_without_a_caller_mapping() {
    let mut game = game();
    let imported = game
        .restore_vanilla_papyrus(&save(0x800, 0, [0, 0, 1]))
        .expect("RefID maps through save table");
    assert_eq!(imported.instances.len(), 1);
    let object = game.object_of_form(soe_esm::GlobalFormId {
        file: 0,
        object: 0x800,
    });
    assert_eq!(
        game.world().form_of(object),
        Some(soe_esm::GlobalFormId {
            file: 0,
            object: 0x800
        })
    );
}

#[test]
fn invalid_bound_refid_fails_without_creating_a_form_object() {
    let mut game = game();
    let error = game
        .restore_vanilla_papyrus(&save(0x800, 0, [0, 0, 2]))
        .unwrap_err();
    assert!(matches!(error, RestoreError::Reference(2)));
    assert_eq!(
        game.world()
            .object_id(soe_game::Handle::Form(soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            })),
        None
    );
}

#[test]
fn signed_minus_one_ignores_even_an_invalid_refid() {
    let mut game = game();
    let imported = game
        .restore_vanilla_papyrus(&save(0x800, u16::MAX, [0, 0, 0x7f]))
        .expect("unbound instance has no form binding");
    assert_eq!(imported.instances.len(), 1);
    assert_eq!(
        game.world()
            .object_id(soe_game::Handle::Form(soe_esm::GlobalFormId {
                file: 0,
                object: 0x800
            })),
        None
    );
}
