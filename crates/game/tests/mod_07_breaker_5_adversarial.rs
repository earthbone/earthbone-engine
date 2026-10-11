// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");
const KEYWORD: u32 = 0x0012DB;
const ARMOR: u32 = 0x816;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "soe-moddata-mod-07-breaker-5-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary fixture directory");
        Self(path)
    }

    fn roots(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

#[test]
fn mod_07_reapplying_real_kid_rule_keeps_one_keyword_and_reports_no_duplicate() {
    // Source: DeviousDevicesNG Devious Devices_KID.ini at revision
    // 6b89b4160fffe62e38c9d3d777e66fb3a06b8419.
    let dir = Dir::new();
    let source = "Devious Devices_KID.ini";
    std::fs::write(
        dir.0.join(source),
        "Keyword = 0x0012DB~Schlongs of Skyrim - Core.esm|Armor|zadNG_SoSRevealing\n",
    )
    .expect("write KID fixture");

    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(EDID, "zadNG_SoSRevealing"));
    plugin.push_top(Record::new(T(b"ARMO"), ARMOR).zsub(EDID, "zadNG_SoSRevealing"));
    let mut order = LoadOrder::new();
    order
        .add(
            "Schlongs of Skyrim - Core.esm",
            Plugin::parse(&plugin.to_bytes()).expect("parse fixture plugin"),
        )
        .expect("add fixture plugin");
    let mut game = Game::new(order);

    let first = game.apply_mod_data(&dir.roots());
    let second = game.apply_mod_data(&dir.roots());

    assert_eq!(first.keyword_rules, 1);
    assert_eq!(first.keywords_attached, 1);
    assert_eq!(second.keyword_rules, 1);
    assert_eq!(second.keywords_attached, 0);
    assert_eq!(
        game.world().keywords(id(ARMOR)),
        vec![id(KEYWORD)],
        "reapplying one real KID rule keeps a single game-visible keyword"
    );
}
