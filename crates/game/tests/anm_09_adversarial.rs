// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{
    Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag,
    plugin::flags,
};
use soe_game::Game;
use soe_game::npc::{self, Config, Stats};
use std::path::PathBuf;

const BASE: u32 = 0x800;
const CELL: u32 = 0x801;
const ACTOR: u32 = 0x802;

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "soe-anm-09-adversarial-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("fixture directory");
        Self(path)
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent directory")).expect("fixture parents");
        std::fs::write(&path, text).expect("fixture file");
        path
    }

    fn roots(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn build_game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(npc::build_record(
        BASE,
        "OarTestActor",
        "OAR Test Actor",
        Config {
            flags: npc::flags::FEMALE,
            ..Config::default()
        },
        Stats::default(),
    ));
    let cell = plugin
        .push_record(Record::new(Tag::new(b"CELL"), CELL).zsub(Tag::new(b"EDID"), "OarTestCell"));
    let actor = plugin.push_record(
        Record::new(Tag::new(b"ACHR"), ACTOR)
            .sub(Tag::new(b"NAME"), BASE.to_le_bytes())
            .sub(Tag::new(b"DATA"), vec![0; 24]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(actor));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut sub_block = Group::new(GroupKind::InteriorSubBlock(0));
    sub_block.entries.push(Entry::Record(cell));
    sub_block.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub_block));
    let mut top = Group::new(GroupKind::Top(Tag::new(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order
        .add(
            "OarTest.esm",
            Plugin::parse(&plugin.to_bytes()).expect("plugin parses"),
        )
        .expect("plugin loads");
    Game::new(order)
}

fn actor_id() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: ACTOR,
    }
}

fn submod(fixture: &Fixture, name: &str, priority: i32, condition: &str) -> PathBuf {
    let directory = format!("meshes/actors/character/animations/OpenAnimationReplacer/Test/{name}");
    fixture.write(
        &format!("{directory}/config.json"),
        &format!(r#"{{"name":"{name}","priority":{priority},"conditions":[{condition}]}}"#),
    );
    fixture.write(&format!("{directory}/walk.hkx"), "synthetic HKX path");
    fixture.0.join(directory)
}

#[test]
fn a_bad_sibling_submod_does_not_hide_a_valid_replacement() {
    let fixture = Fixture::new("bad-sibling");
    let expected = submod(&fixture, "Valid", 10, r#"{"condition":"IsFemale"}"#).join("walk.hkx");
    fixture.write(
        "meshes/actors/character/animations/OpenAnimationReplacer/Test/Broken/config.json",
        "{ invalid json",
    );
    let mut game = build_game();

    game.apply_mod_data(&fixture.roots());

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "an invalid sibling config should not disable another valid submod"
    );
}

#[test]
fn oar_selection_is_repeatable_and_missing_actor_has_no_replacement() {
    let fixture = Fixture::new("repeat");
    let expected =
        submod(&fixture, "FemaleWalk", 20, r#"{"condition":"IsFemale"}"#).join("walk.hkx");
    let mut game = build_game();
    game.apply_mod_data(&fixture.roots());

    for _ in 0..3 {
        assert_eq!(
            game.select_oar_animation(actor_id(), "walk.hkx"),
            Some(expected.clone())
        );
    }
    assert_eq!(
        game.select_oar_animation(
            soe_esm::GlobalFormId {
                file: 0,
                object: 0xFFFF,
            },
            "walk.hkx"
        ),
        None
    );
}
