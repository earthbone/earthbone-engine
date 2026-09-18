// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Regression tests for the two places the body layout is easy to get wrong.
//! Both cost a full parse of every real save when they were wrong, and both
//! fail silently rather than loudly, so they are pinned here.

use soe_save::{Compression, Header, Save, WString};

fn header(width: u32, height: u32, compression: Compression) -> Header {
    Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Prisoner"),
        player_level: 1,
        player_location: WString::from_text("Skyrim"),
        game_date: WString::from_text("000.04.44"),
        player_race: WString::from_text("NordRace"),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 100.0,
        filetime: 0x01D6_FCD1_8C77_6E50,
        screenshot_width: width,
        screenshot_height: height,
        compression,
        screenshot: vec![0; (width * height * 4) as usize],
    }
}

/// The header block is the magic, the size field, the fields the size field
/// covers, and then the screenshot. The screenshot sits outside the declared
/// size, and Special Edition writes four bytes per pixel.
///
/// Getting this length wrong moves where the body starts, which does not fail
/// at the header at all. It fails later as a garbage location table, or as a
/// decompression that happens to succeed on the wrong bytes.
#[test]
fn the_header_block_ends_after_the_screenshot() {
    let header = header(320, 192, Compression::Lz4);
    let mut bytes = Vec::new();
    header.write(&mut bytes);

    // 13 magic, 4 size, 79 fields, then 320 by 192 pixels at four bytes.
    assert_eq!(bytes.len(), 13 + 4 + 79 + 320 * 192 * 4);
    let declared = u32::from_le_bytes(bytes[13..17].try_into().unwrap());
    assert_eq!(
        declared, 79,
        "the declared size covers version through compression only"
    );
}

/// The plugin lists sit behind a `uint32` byte length that the UESP table
/// lists but is easy to read past, since the count immediately after it looks
/// like a plausible plugin count on its own.
///
/// Skipping it shifts every later field by four bytes. The file still
/// decompresses, the form version still looks valid, and the failure only
/// appears as a location table full of nonsense.
#[test]
fn the_plugin_section_is_behind_a_byte_length() {
    let mut plugin_info = Vec::new();
    plugin_info.push(2u8);
    WString::from_text("Skyrim.esm").write(&mut plugin_info);
    WString::from_text("Update.esm").write(&mut plugin_info);
    plugin_info.extend_from_slice(&0u16.to_le_bytes());

    let mut body = Vec::new();
    body.push(78u8);
    body.extend_from_slice(&(plugin_info.len() as u32).to_le_bytes());
    body.extend_from_slice(&plugin_info);
    // A location table of all zeroes is enough to reach the size check.
    body.extend_from_slice(&[0u8; 100]);

    let mut file = Vec::new();
    header(1, 1, Compression::None).write(&mut file);
    file.extend_from_slice(&body);

    // The parse gets past the plugin section and fails later on the empty
    // table, which is what proves the section itself was read correctly.
    let error = Save::parse(&file).expect_err("the zeroed table cannot resolve");
    assert!(
        !error.to_string().contains("plugin section"),
        "the plugin section should have read cleanly, got: {error}"
    );

    // Dropping the length field shifts every later read by four bytes. The
    // first four bytes of the plugin list are then taken as the length, which
    // is a large number, so the parse fails rather than carrying on with
    // silently wrong offsets. Either the section size check or a truncation
    // catches it, depending on what the shifted bytes happen to say.
    let mut broken = Vec::new();
    header(1, 1, Compression::None).write(&mut broken);
    broken.push(78u8);
    broken.extend_from_slice(&plugin_info);
    broken.extend_from_slice(&[0u8; 100]);
    Save::parse(&broken).expect_err("without the length the section is misread");
}
