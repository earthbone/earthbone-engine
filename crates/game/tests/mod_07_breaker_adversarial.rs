// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_moddata::kid::KidFile;
use std::path::PathBuf;

const KEYWORD: u32 = 0x0012DB;
const ARMOR: u32 = 0x0816;

/// The real `Devious Devices_KID.ini` from a DeviousDevicesNG checkout,
/// named by `SOE_TEST_KID_SOURCE`. The tests skip when it is unset.
fn source_file() -> Option<PathBuf> {
    std::env::var_os("SOE_TEST_KID_SOURCE").map(PathBuf::from)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag::new(b"KYWD"), KEYWORD).zsub(tag::EDID, "zadNG_SoSRevealing"));
    plugin.push_top(Record::new(Tag::new(b"ARMO"), ARMOR).zsub(tag::EDID, "zadNG_SoSRevealing"));
    let mut order = LoadOrder::new();
    order
        .add(
            "Schlongs of Skyrim - Core.esm",
            Plugin::parse(&plugin.to_bytes()).expect("parse keyword plugin"),
        )
        .expect("add keyword plugin");
    Game::new(order)
}

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

#[test]
fn mod_07_real_kid_source_keeps_path_line_and_changes_game_keyword_query() {
    // Source: DeviousDevicesNG at 6b89b4160fffe62e38c9d3d777e66fb3a06b8419, Apache-2.0.
    let Some(source) = source_file() else {
        return;
    };
    assert!(
        source.is_file(),
        "missing local real KID source: {}",
        source.display()
    );
    let parsed = KidFile::read(&source).expect("parse real KID source");
    assert_eq!(parsed.path, source);
    assert_eq!(parsed.rules.len(), 1);
    assert_eq!(parsed.rules[0].line, 1);

    let mut game = game();
    let report = game.apply_mod_data(&[source.parent().unwrap().to_path_buf()]);

    assert_eq!(report.keyword_rules, 1);
    assert_eq!(report.keywords_attached, 1);
    assert!(game.world_mut().has_keyword(id(ARMOR), id(KEYWORD)));
}

#[test]
fn mod_07_reapplying_real_kid_source_does_not_duplicate_the_keyword() {
    let Some(source) = source_file() else {
        return;
    };
    assert!(
        source.is_file(),
        "missing local real KID source: {}",
        source.display()
    );
    let mut game = game();
    let root = source.parent().unwrap().to_path_buf();

    let first = game.apply_mod_data(std::slice::from_ref(&root));
    let second = game.apply_mod_data(&[root]);

    assert_eq!(first.keywords_attached, 1);
    assert_eq!(second.keyword_rules, 1);
    assert_eq!(second.keywords_attached, 0);
    assert_eq!(game.world().keywords(id(ARMOR)), vec![id(KEYWORD)]);
}
