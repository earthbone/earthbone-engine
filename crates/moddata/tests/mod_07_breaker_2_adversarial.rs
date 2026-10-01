// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_moddata::{ini::Ini, kid::KidFile, resolve_kid, Change, FormRef};

const KYWD: Tag = Tag::new(b"KYWD");

#[test]
fn mod_07_real_kid_rule_resolves_only_inside_its_named_plugin() {
    // Source: DeviousDevicesNG Devious Devices_KID.ini at revision
    // 6b89b4160fffe62e38c9d3d777e66fb3a06b8419, licensed Apache-2.0.
    let file = KidFile::from_ini(&Ini::parse(
        "Devious Devices_KID.ini",
        "Keyword = 0x0012DB~Schlongs of Skyrim - Core.esm|Armor|zadNG_SoSRevealing",
    ));
    let mut unrelated = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    unrelated.push_top(Record::new(KYWD, 0x12db).zsub(tag::EDID, "WrongPluginKeyword"));
    let mut source = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    source.push_top(Record::new(KYWD, 0x12db).zsub(tag::EDID, "zadNG_SoSRevealing"));
    let mut order = LoadOrder::new();
    order
        .add("Unrelated.esm", Plugin::parse(&unrelated.to_bytes()).unwrap())
        .unwrap();
    order
        .add(
            "Schlongs of Skyrim - Core.esm",
            Plugin::parse(&source.to_bytes()).unwrap(),
        )
        .unwrap();

    let patches = resolve_kid(&file, &order);
    let keyword = FormRef::parse("0x0012DB~Schlongs of Skyrim - Core.esm")
        .resolve(&order)
        .unwrap();
    assert_eq!(
        keyword,
        GlobalFormId {
            file: 1,
            object: 0x12db,
        }
    );
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].change, Change::AddKeyword { keyword });
    assert_eq!(patches[0].source.to_str(), Some("Devious Devices_KID.ini"));
    assert_eq!(patches[0].line, 1);
}
