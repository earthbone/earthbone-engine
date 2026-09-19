// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Synthetic saves, built by the test and read back.
//!
//! A real save cannot live in this repository, so the round trip is proven on
//! files this test writes. Byte equality after a parse and a write is the
//! check that matters: it fails the moment a field is decoded at the wrong
//! width or a section is written back in the wrong order, which is the whole
//! risk in a format this dense.

use soe_save::change::{quest_payload, ChangeForm};
use soe_save::{Compression, GlobalVariable, GlobalVariables, QuestStage, RefId, Save, WString};

/// Builds a save with one quest change form and a few globals.
fn build(compression: Compression) -> Vec<u8> {
    let mut body = Vec::new();

    // Form version, then the plugin lists behind their own byte length.
    body.push(78);
    let mut plugin_info = Vec::new();
    plugin_info.push(1);
    WString::from_text("Skyrim.esm").write(&mut plugin_info);
    plugin_info.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&(plugin_info.len() as u32).to_le_bytes());
    body.extend_from_slice(&plugin_info);

    // The tables are laid out after the location table, whose own size is
    // fixed at 100 bytes, so the offsets can be computed once the pieces are
    // built. Build them first.
    let globals = GlobalVariables {
        variables: vec![
            GlobalVariable {
                form: ref_id(0x41, 0xC0, 0xF2),
                value: 3.0,
            },
            GlobalVariable {
                form: ref_id(0x40, 0x00, 0x2A),
                value: 1.5,
            },
        ],
    };
    let mut table_1 = Vec::new();
    let global_block = globals.write().unwrap();
    table_1.extend_from_slice(&3u32.to_le_bytes());
    table_1.extend_from_slice(&(global_block.len() as u32).to_le_bytes());
    table_1.extend_from_slice(&global_block);

    let stages = [
        QuestStage {
            stage: 10,
            status: 1,
        },
        QuestStage {
            stage: 20,
            status: 1,
        },
        QuestStage {
            stage: 30,
            status: 0,
        },
    ];
    let payload = quest_payload(&stages).unwrap();
    let mut change_forms = Vec::new();
    // Type 8 is QUST, width 0 means one byte lengths, flag bit 31 says the
    // payload carries stages.
    let form = ChangeForm {
        form: ref_id(0x40, 0x00, 0x99),
        change_flags: 1 << 31,
        type_field: 8,
        version: 74,
        uncompressed_len: 0,
        data: payload,
    };
    form.write(&mut change_forms);

    // Table three carries a Papyrus prefix through the second pass: version,
    // empty strings, scripts, instances, references, arrays and active scripts.
    let mut papyrus_block = Vec::new();
    papyrus_block.extend_from_slice(&2u16.to_le_bytes());
    papyrus_block.extend_from_slice(&0u32.to_le_bytes());
    papyrus_block.extend_from_slice(&0u32.to_le_bytes());
    papyrus_block.extend_from_slice(&0u32.to_le_bytes());
    papyrus_block.extend_from_slice(&0u32.to_le_bytes());
    papyrus_block.extend_from_slice(&0u32.to_le_bytes());
    papyrus_block.extend_from_slice(&0u32.to_le_bytes()); // runtime ID
    papyrus_block.extend_from_slice(&0u32.to_le_bytes());
    let mut table_3 = Vec::new();
    table_3.extend_from_slice(&1001u32.to_le_bytes());
    table_3.extend_from_slice(&(papyrus_block.len() as u32).to_le_bytes());
    table_3.extend_from_slice(&papyrus_block);

    let form_ids: [u32; 2] = [0x0001_C0F2, 0x0002_0000];

    // Offsets are absolute, so they include the header block length, which is
    // filled in after the header is built. Start from the body position.
    let header_len = header_block(compression).len();
    let table_1_at = body.len() + 100;
    let table_2_at = table_1_at + table_1.len();
    let change_forms_at = table_2_at;
    let table_3_at = change_forms_at + change_forms.len();
    let form_id_at = table_3_at + table_3.len();

    let mut table = Vec::new();
    for value in [
        (header_len + form_id_at) as u32,
        (header_len + form_id_at) as u32,
        (header_len + table_1_at) as u32,
        (header_len + table_2_at) as u32,
        (header_len + change_forms_at) as u32,
        (header_len + table_3_at) as u32,
        1, // table 1 count
        0, // table 2 count
        0, // table 3 count, short by the Papyrus block as the game writes it
        1, // change form count
    ] {
        table.extend_from_slice(&value.to_le_bytes());
    }
    table.extend_from_slice(&[0u8; 60]);
    assert_eq!(table.len(), 100, "the location table is a fixed 100 bytes");

    body.extend_from_slice(&table);
    body.extend_from_slice(&table_1);
    body.extend_from_slice(&change_forms);
    body.extend_from_slice(&table_3);
    body.extend_from_slice(&(form_ids.len() as u32).to_le_bytes());
    for id in form_ids {
        body.extend_from_slice(&id.to_le_bytes());
    }
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(b"tail");

    let mut file = header_block(compression);
    match compression {
        Compression::None => file.extend_from_slice(&body),
        Compression::Lz4 => {
            let compressed = lz4_compress(&body);
            file.extend_from_slice(&(body.len() as u32).to_le_bytes());
            file.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
            file.extend_from_slice(&compressed);
        }
        Compression::Zlib => panic!("the fixture does not build zlib bodies"),
    }
    file
}

fn lz4_compress(data: &[u8]) -> Vec<u8> {
    // The crate under test uses lz4_flex block compression, and the fixture
    // has to produce a body it can inflate with a known length.
    lz4_flex::block::compress(data)
}

fn ref_id(a: u8, b: u8, c: u8) -> RefId {
    RefId::from_bytes([a, b, c])
}

fn header_block(compression: Compression) -> Vec<u8> {
    let header = soe_save::Header {
        version: 12,
        save_number: 2,
        player_name: WString::from_text("Dragonborn"),
        player_level: 5,
        player_location: WString::from_text("Riverwood"),
        game_date: WString::from_text("1.20.4"),
        player_race: WString::from_text("NordRace"),
        player_sex: 0,
        current_experience: 12.0,
        needed_experience: 100.0,
        filetime: 0x01D6_0000_0000_0000,
        screenshot_width: 2,
        screenshot_height: 2,
        compression,
        screenshot: vec![0xAB; 2 * 2 * 4],
    };
    let mut out = Vec::new();
    header.write(&mut out);
    out
}

#[test]
fn an_uncompressed_save_round_trips_byte_for_byte() {
    let bytes = build(Compression::None);
    let save = Save::parse(&bytes).expect("parses");
    assert_eq!(save.header.player_name.to_text(), "Dragonborn");
    assert_eq!(save.header.player_level, 5);
    assert_eq!(save.change_forms.len(), 1);
    let written = save.write().expect("writes");
    assert_eq!(written, bytes, "an uncompressed save must round trip");
}

#[test]
fn a_compressed_save_round_trips_through_its_body() {
    let bytes = build(Compression::Lz4);
    let save = Save::parse(&bytes).expect("parses");
    assert_eq!(save.header.compression, Compression::Lz4);
    assert_eq!(save.change_forms.len(), 1);
    // The compressed bytes depend on the encoder, so compare the bodies by
    // parsing the rewritten file rather than the files themselves.
    let written = save.write().expect("writes");
    let again = Save::parse(&written).expect("reparses");
    assert_eq!(again.change_forms.len(), save.change_forms.len());
    assert_eq!(again.form_id_array, save.form_id_array);
    assert_eq!(again.unknown_3, save.unknown_3);
}

#[test]
fn quest_stages_and_globals_come_back_out() {
    let bytes = build(Compression::None);
    let save = Save::parse(&bytes).expect("parses");

    let stages = save.quest_stages().expect("decodes stages");
    assert_eq!(stages.len(), 1, "one quest change form carries stages");
    let (_form, stages) = &stages[0];
    assert_eq!(stages.len(), 3);
    assert!(stages[0].is_done());
    assert!(stages[1].is_done());
    assert!(!stages[2].is_done(), "stage 30 was left not done");

    let globals = save.globals().expect("decodes globals");
    assert_eq!(globals.variables.len(), 2);
    assert!((globals.variables[0].value - 3.0).abs() < f32::EPSILON);
}

#[test]
fn expectations_name_what_they_can_and_comment_the_rest() {
    let bytes = build(Compression::None);
    let save = Save::parse(&bytes).expect("parses");

    let expectations = save
        .to_expectations(|form| match form {
            0x0000_0099 => Some("MQ101".to_string()),
            0x0001_C0F2 => Some("DragonsAbsorbed".to_string()),
            _ => None,
        })
        .expect("renders");

    let text = expectations.render();
    assert!(
        text.contains("expect stage-done MQ101 10 true"),
        "rendered:\n{text}"
    );
    assert!(
        text.contains("expect stage-done MQ101 20 true"),
        "rendered:\n{text}"
    );
    assert!(
        !text.contains("MQ101 30"),
        "a stage that is not done must not be asserted:\n{text}"
    );
    assert!(
        text.contains("expect global DragonsAbsorbed == 3"),
        "rendered:\n{text}"
    );
    assert_eq!(expectations.stages, 2);
    assert_eq!(expectations.globals, 1);
    assert_eq!(expectations.unnamed, 1, "the second global has no name");
    assert!(
        text.contains("# global"),
        "an unnamed form becomes a comment:\n{text}"
    );
}

#[test]
fn a_papyrus_block_is_found_and_read() {
    let bytes = build(Compression::None);
    let save = Save::parse(&bytes).expect("parses");
    let papyrus = save.papyrus().expect("the save has one").expect("parses");
    assert_eq!(papyrus.vm_version, 2);
    assert!(papyrus.strings.is_empty());
    assert!(papyrus.instances.is_empty());
    assert!(papyrus.references.is_empty());
    assert!(papyrus.arrays.is_empty());
    assert!(papyrus.active_scripts.is_empty());
    assert!(papyrus.value_data_bytes().unwrap().is_empty());
    assert_eq!(papyrus.decoded_len, papyrus.block_len);
}

#[test]
fn a_legendary_edition_save_is_refused_by_version() {
    let mut bytes = build(Compression::None);
    // Version sits right after the 13 byte magic and the 4 byte header size.
    bytes[17..21].copy_from_slice(&9u32.to_le_bytes());
    let error = Save::parse(&bytes).expect_err("version 9 is Legendary Edition");
    assert!(
        error.to_string().contains("Special Edition"),
        "got: {error}"
    );
}
