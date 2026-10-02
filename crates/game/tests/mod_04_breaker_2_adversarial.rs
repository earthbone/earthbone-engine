// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::Game;
use soe_game::npc::{self, Config, Stats};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

const BASE: u32 = 0x800;
const CELL: u32 = 0x801;
const ACTOR: u32 = 0x802;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "soe-mod-04-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, relative: &str, text: &str) -> PathBuf {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn actor_game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(npc::build_record(
        BASE,
        "OverrideActor",
        "Override Actor",
        Config {
            flags: npc::flags::FEMALE,
            ..Config::default()
        },
        Stats::default(),
    ));
    plugin.push_top(Record::new(Tag::new(b"RACE"), 0x804));
    let cell = plugin.push_record(Record::new(Tag::new(b"CELL"), CELL));
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
        .add("Override.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn actor_id() -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId {
        file: 0,
        object: ACTOR,
    }
}

fn submod(fixture: &Fixture, name: &str, priority: i32) -> PathBuf {
    let directory = format!("meshes/actors/character/animations/OpenAnimationReplacer/Test/{name}");
    fixture.write(
        &format!("{directory}/config.json"),
        &format!(
            r#"{{"name":"{name}","priority":{priority},"conditions":[{{"condition":"IsFemale"}}]}}"#
        ),
    );
    fixture.write(&format!("{directory}/walk.hkx"), "synthetic HKX path");
    fixture.0.join(directory)
}

#[test]
fn mod_04_user_priority_changes_the_clip_selected_by_the_game() {
    let fixture = Fixture::new();
    let overridden = submod(&fixture, "Author", 2);
    let competitor = submod(&fixture, "Competitor", 8);
    fixture.write(
        &format!(
            "{}/user.json",
            overridden.strip_prefix(&fixture.0).unwrap().display()
        ),
        r#"{"priority":12}"#,
    );
    let mut game = actor_game();
    game.apply_mod_data(std::slice::from_ref(&fixture.0));

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(overridden.join("walk.hkx")),
        "the user priority must put this clip ahead of the higher author priority competitor"
    );
    assert_ne!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(competitor.join("walk.hkx"))
    );
}

#[test]
fn mod_04_user_override_animation_folder_supplies_the_selected_clip() {
    let fixture = Fixture::new();
    let submod = submod(&fixture, "Author", 4);
    let relative = submod.strip_prefix(&fixture.0).unwrap();
    fixture.write(
        &format!("{}/user.json", relative.display()),
        r#"{"priority":4,"overrideAnimationsFolder":"Shared"}"#,
    );
    let expected = fixture.write(
        "meshes/actors/character/animations/OpenAnimationReplacer/Test/Shared/walk.hkx",
        "shared replacement clip",
    );
    let mut game = actor_game();
    game.apply_mod_data(std::slice::from_ref(&fixture.0));

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "OAR resolves overrideAnimationsFolder from the submod parent"
    );
}
