// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_moddata::{
    bos::SwapFile, ini::Ini, kid::KidFile, resolve_bos, resolve_kid, resolve_spid, spid::DistrFile,
};

const KYWD: Tag = Tag::new(b"KYWD");
const SPEL: Tag = Tag::new(b"SPEL");

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(KYWD, 0x800).zsub(tag::EDID, "FixtureKeyword"));
    plugin.push_top(Record::new(SPEL, 0x801).zsub(tag::EDID, "FixtureSpell"));
    plugin.push_top(Record::new(tag::STAT, 0x802).zsub(tag::EDID, "FixtureRock"));
    plugin.push_top(Record::new(tag::STAT, 0x803).zsub(tag::EDID, "FixtureTree"));
    let mut load_order = LoadOrder::new();
    load_order
        .add("Fixture.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    load_order
}

#[test]
fn mod_07_kid_keeps_real_input_line_after_missing_form_is_dropped() {
    let parsed = KidFile::from_ini(&Ini::parse(
        "Fixture_KID.ini",
        "Keyword = Missing|Armor|Iron\n; authored fixture\nKeyword = FixtureKeyword|Armor|Iron\n",
    ));
    let patches = resolve_kid(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source.to_str(), Some("Fixture_KID.ini"));
    assert_eq!(patches[0].line, 3);
}

#[test]
fn mod_07_spid_keeps_real_input_line_after_missing_form_is_dropped() {
    let parsed = DistrFile::from_ini(&Ini::parse(
        "Fixture_DISTR.ini",
        "Spell = Missing|NONE\nSpell = FixtureSpell|NONE\n",
    ));
    let patches = resolve_spid(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source.to_str(), Some("Fixture_DISTR.ini"));
    assert_eq!(patches[0].line, 2);
}

#[test]
fn mod_07_bos_keeps_rule_provenance_when_replacement_is_missing() {
    let parsed = SwapFile::from_ini(&Ini::parse(
        "Fixture_SWAP.ini",
        "[Forms]\nFixtureRock|0x999~Absent.esp|scale(2)\n",
    ));
    let patches = resolve_bos(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source.to_str(), Some("Fixture_SWAP.ini"));
    assert_eq!(patches[0].line, 2);
}
