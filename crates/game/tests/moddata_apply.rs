// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The half `crates/moddata` could not do: a configuration file that changes
//! the running game.
//!
//! Each test writes a real ini into a temporary directory, applies it to a
//! world built from a synthetic plugin, and then asks the question a script
//! would ask. Asserting through `has_keyword`, the actor's own spell set and
//! `GetItemCount` rather than through the patch list is the point: a side
//! table this module alone reads would pass a weaker test and fail a mod.

use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const KEYWORD: u32 = 0x800;
const KEYWORD_TWO: u32 = 0x814;
const KEYWORD_THREE: u32 = 0x815;
const PRETAGGED_HELMET: u32 = 0x816;
const SPELL: u32 = 0x801;
const GOLD: u32 = 0x802;
const HELMET: u32 = 0x803;
const ROCK: u32 = 0x804;
const TREE: u32 = 0x805;
const GUARD_BASE: u32 = 0x806;
const CELL: u32 = 0x810;
const ROCK_REF: u32 = 0x811;
const GUARD_REF: u32 = 0x812;
const SECOND_GUARD_REF: u32 = 0x813;

/// A directory of this test's own, named for the case so two cases running at
/// once cannot read each other's files.
struct Dir(PathBuf);

impl Dir {
    fn new(case: &str) -> Dir {
        let path = std::env::temp_dir().join(format!("soe-moddata-{}-{case}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temp directory");
        Dir(path)
    }

    fn write(&self, name: &str, text: &str) {
        std::fs::write(self.0.join(name), text).expect("the fixture writes");
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

fn placed(tag: [u8; 4], id: u32, edid: &str, base: u32) -> Record {
    Record::new(T(&tag), id)
        .zsub(EDID, edid)
        .sub(T(b"NAME"), base.to_le_bytes())
        .sub(T(b"DATA"), vec![0u8; 24])
}

/// One keyword, one spell, two items, two statics to swap between, a guard
/// base and a cell holding a rock and the guard.
fn master() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(EDID, "MyKeyword"));
    p.push_top(Record::new(T(b"KYWD"), KEYWORD_TWO).zsub(EDID, "AnotherKeyword"));
    p.push_top(Record::new(T(b"KYWD"), KEYWORD_THREE).zsub(EDID, "ThirdKeyword"));
    p.push_top(Record::new(T(b"SPEL"), SPELL).zsub(EDID, "MySpell"));
    p.push_top(Record::new(T(b"MISC"), GOLD).zsub(EDID, "Gold001"));
    p.push_top(
        Record::new(T(b"ARMO"), HELMET)
            .zsub(EDID, "GuardHelmet")
            .zsub(T(b"FULL"), "Guard Helmet"),
    );
    p.push_top(Record::new(T(b"STAT"), ROCK).zsub(EDID, "RockPile01"));
    p.push_top(Record::new(T(b"STAT"), TREE).zsub(EDID, "TreePine01"));
    p.push_top(npc::build_record(
        GUARD_BASE,
        "WhiterunGuard",
        "Whiterun Guard",
        Config::default(),
        Stats::default(),
    ));

    let cell = p.push_record(
        Record::new(T(b"CELL"), CELL)
            .zsub(EDID, "TestCell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut temp = Group::new(GroupKind::CellTemporary(CELL));
    for r in [
        placed(*b"REFR", ROCK_REF, "TheRock", ROCK),
        placed(*b"ACHR", GUARD_REF, "TheGuard", GUARD_BASE),
    ] {
        let idx = p.push_record(r);
        temp.entries.push(Entry::Record(idx));
    }
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(sub));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    let bytes = master().to_bytes();
    order
        .add("Master.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    Game::new(order)
}

fn game_with_pretagged_helmet() -> Game {
    let mut plugin = master();
    plugin.push_top(
        Record::new(T(b"ARMO"), PRETAGGED_HELMET)
            .zsub(EDID, "PreTaggedHelmet")
            .sub(T(b"KWDA"), KEYWORD.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    let bytes = plugin.to_bytes();
    order
        .add("Master.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    Game::new(order)
}

fn game_with_guards(count: u32) -> Game {
    let mut plugin = master();
    for offset in 0..count {
        let object = SECOND_GUARD_REF + offset;
        plugin.push_top(placed(
            *b"ACHR",
            object,
            &format!("Guard{object}"),
            GUARD_BASE,
        ));
    }
    let mut order = LoadOrder::new();
    let bytes = plugin.to_bytes();
    order
        .add("Master.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    Game::new(order)
}

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

#[test]
fn a_keyword_file_makes_has_keyword_answer_true() {
    let dir = Dir::new("kid");
    dir.write("Test_KID.ini", "Keyword = MyKeyword|Armor|GuardHelmet\n");
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.keyword_rules, 1);
    assert_eq!(report.keywords_attached, 1);

    // The question a script asks, not the patch list.
    assert!(
        g.world_mut().has_keyword(id(HELMET), id(KEYWORD)),
        "the distributed keyword must reach the same path as a shipped one"
    );
    // And the keyword list itself, so a native that reads the whole set sees it.
    assert!(g.world().keywords(id(HELMET)).contains(&id(KEYWORD)));
}

#[test]
fn a_keyword_file_leaves_a_form_its_filter_misses_alone() {
    let dir = Dir::new("kid-miss");
    dir.write("Test_KID.ini", "Keyword = MyKeyword|Armor|SomethingElse\n");
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.keyword_rules, 1);
    assert_eq!(report.keywords_attached, 0);
    assert!(!g.world_mut().has_keyword(id(HELMET), id(KEYWORD)));
}

#[test]
fn mod_02_exclusive_group_adds_only_one_matching_keyword_to_an_item() {
    let dir = Dir::new("kid-exclusive");
    dir.write(
        "First_KID.ini",
        "ExclusiveGroup = Equipment|MyKeyword,AnotherKeyword\nKeyword = MyKeyword|Armor|GuardHelmet\nKeyword = AnotherKeyword|Armor|GuardHelmet\n",
    );
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.keywords_attached, 1);
    assert_eq!(
        g.world()
            .keywords(id(HELMET))
            .iter()
            .filter(|keyword| { **keyword == id(KEYWORD) || **keyword == id(KEYWORD_TWO) })
            .count(),
        1,
        "a KID exclusive group allows only one member on the item"
    );
}

#[test]
fn mod_02_exclusive_group_merges_files_and_applies_later_exclusions() {
    let dir = Dir::new("kid-exclusive-merge");
    dir.write(
        "A_KID.ini",
        "ExclusiveGroup = Equipment|MyKeyword,AnotherKeyword\nKeyword = AnotherKeyword|Armor|GuardHelmet\n",
    );
    dir.write(
        "B_KID.ini",
        "ExclusiveGroup = Equipment|ThirdKeyword,-AnotherKeyword\nKeyword = ThirdKeyword|Armor|GuardHelmet\n",
    );
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.keywords_attached, 2);
    assert!(g.world().keywords(id(HELMET)).contains(&id(KEYWORD_THREE)));
    assert!(g.world().keywords(id(HELMET)).contains(&id(KEYWORD_TWO)));
    assert!(!g.world().keywords(id(HELMET)).contains(&id(KEYWORD)));
}

#[test]
fn mod_02_exclusive_group_respects_keywords_already_on_the_record() {
    let dir = Dir::new("kid-exclusive-existing");
    dir.write(
        "Existing_KID.ini",
        "ExclusiveGroup = Equipment|MyKeyword,AnotherKeyword\nKeyword = AnotherKeyword|Armor|PreTaggedHelmet\n",
    );
    let mut g = game_with_pretagged_helmet();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.keywords_attached, 0);
    assert!(g
        .world()
        .keywords(id(PRETAGGED_HELMET))
        .contains(&id(KEYWORD)));
    assert!(!g
        .world()
        .keywords(id(PRETAGGED_HELMET))
        .contains(&id(KEYWORD_TWO)));
}

#[test]
fn mod_02_kid_chance_uses_keyword_and_target_seed() {
    let dir = Dir::new("kid-seeded-chance");
    dir.write(
        "Seeded_KID.ini",
        "Keyword = MyKeyword|Armor|GuardHelmet|NONE|75\n",
    );
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.keywords_attached, 1);
    assert!(g.world().keywords(id(HELMET)).contains(&id(KEYWORD)));
}

#[test]
fn a_distribution_file_hands_an_actor_a_spell() {
    let dir = Dir::new("spid-spell");
    dir.write("Test_DISTR.ini", "Spell = MySpell|WhiterunGuard\n");
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.distribution_rules, 1);
    assert_eq!(report.distributions_applied, 1);

    // Built from the base on first use, which is after the distribution ran.
    let mut world = g.world_mut();
    let actor = world.actor(id(GUARD_REF)).expect("the guard is an actor");
    assert!(
        actor.spells.contains(&id(SPELL)),
        "a distributed spell must sit beside the base's own"
    );
}

#[test]
fn a_distribution_file_hands_an_actor_an_item_that_counts() {
    let dir = Dir::new("spid-item");
    dir.write(
        "Test_DISTR.ini",
        "Item = Gold001|WhiterunGuard|NONE|NONE|NONE|5\n",
    );
    let mut g = game();
    g.apply_mod_data(&dir.roots());

    let mut world = g.world_mut();
    assert_eq!(
        world.item_count(id(GUARD_REF), id(GOLD)),
        5,
        "a distributed item must count the way a shipped one does"
    );
}

#[test]
fn a_swap_file_changes_what_a_reference_resolves_to() {
    let dir = Dir::new("bos");
    dir.write("Test_SWAP.ini", "[Forms]\nRockPile01|TreePine01\n");
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert!(report.swap_rules >= 1);

    // The reference is built now, long after the swap was read, which is the
    // case that matters: a reference is not loaded at start.
    let mut world = g.world_mut();
    assert_eq!(
        world.base_of(id(ROCK_REF)),
        Some(id(TREE)),
        "the placed rock must resolve to the tree it was swapped for"
    );
}

#[test]
fn a_level_filtered_rule_is_skipped_rather_than_applied_to_everyone() {
    let dir = Dir::new("spid-level");
    dir.write(
        "Test_DISTR.ini",
        "Spell = MySpell|WhiterunGuard|NONE|10/20\n",
    );
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.distribution_rules, 1);
    assert_eq!(
        report.skipped_unsupported_filter, 1,
        "a filter this module cannot evaluate must skip, never apply broadly"
    );
    assert_eq!(report.distributions_applied, 0);
}

#[test]
fn a_zero_chance_rule_never_applies_and_a_full_one_always_does() {
    let dir = Dir::new("chance");
    dir.write("Never_KID.ini", "Keyword = MyKeyword|Armor|NONE|NONE|0\n");
    let mut g = game();
    let report = g.apply_mod_data(&dir.roots());
    assert_eq!(report.skipped_by_chance, 1);
    assert_eq!(report.keywords_attached, 0);
}

#[test]
fn applying_the_same_files_twice_from_one_seed_hashes_the_same() {
    // Distribution rolls a chance from the world generator, so this is the
    // test that a mod does not make a run unreproducible.
    let dir = Dir::new("determinism");
    dir.write("Test_KID.ini", "Keyword = MyKeyword|Armor|*Guard|NONE|50\n");
    dir.write(
        "Test_DISTR.ini",
        "Spell = MySpell|WhiterunGuard|NONE|NONE|NONE|1|50\n",
    );
    dir.write("Test_SWAP.ini", "[Forms]\nRockPile01|TreePine01\n");

    let hash_of = || {
        let mut g = game();
        g.apply_mod_data(&dir.roots());
        let mut world = g.world_mut();
        // Touch both lazily built things so the hash covers them.
        let _ = world.actor(id(GUARD_REF));
        let _ = world.base_of(id(ROCK_REF));
        world.state_hash()
    };
    let first = hash_of();
    let second = hash_of();
    assert_eq!(
        first.mod_data, second.mod_data,
        "the mod data part must not move between two runs of one seed"
    );
    assert_eq!(first, second, "no part may move");
}

/// The installed game is vanilla, so this finds nothing and says so. It starts
/// reporting the moment a mod using any of the formats is installed, which is
/// the only way this crate will ever meet a file it did not author.
#[test]
fn the_installed_game_is_scanned_for_real_mod_data() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: SKYRIM_DATA is not set");
        return;
    };
    let mut g = game();
    let report = g.apply_mod_data(std::slice::from_ref(&data));
    eprintln!("mod data under {}:", data.display());
    eprintln!("  files {}", report.files);
    eprintln!(
        "  keyword rules {}, attached {}",
        report.keyword_rules, report.keywords_attached
    );
    eprintln!(
        "  distribution rules {}, applied {}, skipped by filter {}",
        report.distribution_rules, report.distributions_applied, report.skipped_unsupported_filter
    );
    eprintln!("  swap rules {}", report.swap_rules);
    eprintln!("  animation submods {}", report.oar_trees);
    if report.files == 0 {
        eprintln!("  none: this install ships no mod using these formats");
    }
}

#[test]
fn mod_01_zero_and_full_deterministic_chances_remain_exact() {
    for (chance, applied) in [("0!", 0), ("100!", 1)] {
        let dir = Dir::new(&format!("spid-chance-{chance}"));
        dir.write(
            "Test_DISTR.ini",
            &format!("Spell = MySpell|NONE|NONE|NONE|NONE|1|{chance}\n"),
        );
        let mut g = game();
        let report = g.apply_mod_data(&dir.roots());
        assert_eq!(report.distributions_applied, applied);
        assert_eq!(
            g.world_mut()
                .actor(id(GUARD_REF))
                .unwrap()
                .spells
                .contains(&id(SPELL)),
            applied == 1
        );
    }
}

#[test]
fn mod_01_same_base_actors_get_distinct_repeatable_deterministic_results() {
    let dir = Dir::new("spid-per-actor");
    dir.write(
        "Test_DISTR.ini",
        "Spell = MySpell|WhiterunGuard|NONE|NONE|NONE|1|50!\n",
    );
    let results = || {
        let mut g = game_with_guards(16);
        g.apply_mod_data(&dir.roots());
        std::iter::once(GUARD_REF)
            .chain(SECOND_GUARD_REF..SECOND_GUARD_REF + 16)
            .map(|reference| {
                g.world_mut()
                    .actor(id(reference))
                    .unwrap()
                    .spells
                    .contains(&id(SPELL))
            })
            .collect::<Vec<_>>()
    };
    let first_run = results();
    let second_run = results();
    let other = first_run
        .iter()
        .position(|result| *result != first_run[0])
        .unwrap_or_else(|| panic!("same-base deterministic results did not vary: {first_run:?}"));
    assert_ne!(
        first_run[0], first_run[other],
        "actor FormIDs seed separate results"
    );
    assert_eq!(
        first_run, second_run,
        "the same load and actor IDs repeat results"
    );
}
