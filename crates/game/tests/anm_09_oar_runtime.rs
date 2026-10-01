// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const CELL: u32 = 0x801;
const ACTOR: u32 = 0x802;

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("soe-anm-09-{}-{label}", std::process::id()));
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
    plugin.push_top(Record::new(T(b"RACE"), 0x804));
    let cell =
        plugin.push_record(Record::new(T(b"CELL"), CELL).zsub(Tag::new(b"EDID"), "OarTestCell"));
    let actor = plugin.push_record(
        Record::new(T(b"ACHR"), ACTOR)
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
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
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

fn actor_id() -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId {
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

fn female_condition() -> &'static str {
    r#"{"condition":"IsFemale"}"#
}

#[test]
fn anm_09_passing_condition_selects_a_replacement_clip() {
    let fixture = Fixture::new("select");
    let expected = submod(&fixture, "FemaleWalk", 20, female_condition()).join("walk.hkx");
    let mut game = build_game();
    let report = game.apply_mod_data(&fixture.roots());

    assert_eq!(report.oar_trees, 1, "the submod is discovered");
    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "a passing OAR condition selects its clip file"
    );
}

#[test]
fn anm_09_actor_base_condition_resolves_its_plugin_form() {
    let fixture = Fixture::new("actor-base");
    let expected = submod(
        &fixture,
        "BaseWalk",
        20,
        r#"{"condition":"IsActorBase","form":{"pluginName":"OarTest.esm","formID":"0x800"}}"#,
    )
    .join("walk.hkx");
    let mut game = build_game();
    game.apply_mod_data(&fixture.roots());

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "the actor base condition resolves against the active load order"
    );
}

#[test]
fn anm_09_priority_and_unsupported_conditions_control_selection() {
    let fixture = Fixture::new("priority");
    submod(&fixture, "Lower", 5, female_condition());
    let expected = submod(&fixture, "Higher", 50, female_condition()).join("walk.hkx");
    let mut game = build_game();
    game.apply_mod_data(&fixture.roots());
    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "the highest-priority passing submod wins"
    );

    let no_match = Fixture::new("unsupported");
    submod(
        &no_match,
        "UnknownCondition",
        100,
        r#"{"condition":"PluginCondition","requiredPlugin":"missing.dll"}"#,
    );
    let mut game = build_game();
    game.apply_mod_data(&no_match.roots());
    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        None,
        "unsupported plugin conditions cannot activate a clip"
    );

    let malformed = Fixture::new("malformed");
    submod(&malformed, "MalformedCondition", 200, "null");
    let mut game = build_game();
    game.apply_mod_data(&malformed.roots());
    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        None,
        "a malformed condition cannot turn its submod into an unconditional match"
    );
}

#[test]
fn anm_09_legacy_or_condition_group_selects_when_one_branch_passes() {
    let fixture = Fixture::new("legacy-or");
    let directory =
        "meshes/actors/character/animations/DynamicAnimationReplacer/_CustomConditions/10";
    fixture.write(
        &format!("{directory}/_conditions.txt"),
        "IsFemale() OR\nPluginCondition()\n",
    );
    let expected = fixture.write(&format!("{directory}/walk.hkx"), "synthetic HKX path");
    let mut game = build_game();
    game.apply_mod_data(&fixture.roots());

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "a passing branch satisfies its legacy OR group"
    );
}

#[test]
fn anm_09_legacy_actor_base_form_selects_its_clip() {
    let fixture = Fixture::new("legacy-base");
    let directory =
        "meshes/actors/character/animations/DynamicAnimationReplacer/_CustomConditions/10";
    fixture.write(
        &format!("{directory}/_conditions.txt"),
        "IsActorBase(\"OarTest.esm\"|0x800)\n",
    );
    let expected = fixture.write(&format!("{directory}/walk.hkx"), "synthetic HKX path");
    let mut game = build_game();
    game.apply_mod_data(&fixture.roots());

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "a legacy actor base form resolves through its named plugin"
    );
}

#[test]
fn anm_09_unavailable_required_plugin_cannot_select_builtin_condition() {
    let fixture = Fixture::new("required-plugin");
    submod(
        &fixture,
        "UnavailablePlugin",
        10,
        r#"{"condition":"IsFemale","requiredPlugin":"missing.dll"}"#,
    );
    let mut game = build_game();
    game.apply_mod_data(&fixture.roots());

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        None,
        "a condition with an unavailable plugin dependency fails closed"
    );
}

#[test]
fn anm_09_is_race_reads_the_actors_current_race() {
    let fixture = Fixture::new("current-race");
    let expected = submod(
        &fixture,
        "ChangedRace",
        10,
        r#"{"condition":"IsRace","form":{"pluginName":"OarTest.esm","formID":"0x804"}}"#,
    )
    .join("walk.hkx");
    let mut game = build_game();
    game.world_mut().actor(actor_id()).unwrap().race = Some(soe_esm::GlobalFormId {
        file: 0,
        object: 0x804,
    });
    game.apply_mod_data(&fixture.roots());

    assert_eq!(
        game.select_oar_animation(actor_id(), "walk.hkx"),
        Some(expected),
        "IsRace follows the live actor race rather than the NPC base"
    );
}
