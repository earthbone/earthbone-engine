// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_moddata::{ini::Ini, resolve_spid, spid::DistrFile};
use std::path::PathBuf;

const SPEL: Tag = Tag::new(b"SPEL");

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(SPEL, 0x801).zsub(tag::EDID, "FixtureSpell"));
    let mut order = LoadOrder::new();
    order
        .add("Fixture.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

#[test]
fn mod_07_spid_accepts_form_id_hyphen_plugin_spelling() {
    let file = DistrFile::from_ini(&Ini::parse(
        "RealStyle_DISTR.ini",
        "Spell = 0x801 - Fixture.esm|NONE\n",
    ));

    let patches = resolve_spid(&file, &order());
    assert_eq!(patches.len(), 1, "the shipped SPID spelling resolves");
    assert_eq!(patches[0].source, PathBuf::from("RealStyle_DISTR.ini"));
    assert_eq!(patches[0].line, 1);
}

#[test]
fn mod_07_spid_resolution_keeps_full_fixture_path_and_source_line() {
    let directory = std::env::temp_dir().join(format!(
        "mod-07-provenance-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("Example_DISTR.ini");
    std::fs::write(
        &path,
        "Spell = MissingSpell|NONE\n; shipped fixture line\nSpell = FixtureSpell|NONE\n",
    )
    .unwrap();

    let parsed = DistrFile::read(&path).unwrap();
    let patches = resolve_spid(&parsed, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].source, path);
    assert_eq!(patches[0].line, 3);

    std::fs::remove_dir_all(directory).unwrap();
}
