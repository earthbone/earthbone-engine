// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_moddata::{ini::Ini, kid::KidFile, resolve_kid};

const KYWD: Tag = Tag::new(b"KYWD");

#[test]
fn mod_07_real_qualified_keyword_does_not_bind_to_same_id_in_another_plugin() {
    // Source: DeviousDevicesNG Devious Devices_KID.ini, revision
    // 6b89b4160fffe62e38c9d3d777e66fb3a06b8419, Apache-2.0.
    // KID parses the explicit file and form pair as RawForm.
    let file = KidFile::from_ini(&Ini::parse(
        "Devious Devices_KID.ini",
        "Keyword = 0x0012DB~Schlongs of Skyrim - Core.esm|Armor|zadNG_SoSRevealing",
    ));

    let mut unrelated = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    unrelated.push_top(Record::new(KYWD, 0x12db).zsub(tag::EDID, "SoSRevealing"));
    let mut order = LoadOrder::new();
    order
        .add(
            "Unrelated.esm",
            Plugin::parse(&unrelated.to_bytes()).unwrap(),
        )
        .unwrap();

    assert!(
        resolve_kid(&file, &order).is_empty(),
        "a file-qualified FormID must remain unresolved when its named plugin is absent"
    );
}
