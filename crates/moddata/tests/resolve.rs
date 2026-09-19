// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Resolution against a real load order, which is the half the unit tests
//! cannot reach: a parsed configuration names forms as text, and only a load
//! order turns that text into a form id or proves it absent.

use soe_esm::{plugin::flags, tag, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_moddata::{
    bos::SwapFile, ini::Ini, kid::KidFile, resolve_bos, resolve_kid, resolve_spid, spid::DistrFile,
    Change, FormRef,
};

// The esm crate names only the tags its readers need, so the two this fixture
// wants are spelled here rather than added to that crate.
const KYWD: Tag = Tag::new(b"KYWD");
const SPEL: Tag = Tag::new(b"SPEL");

const KEYWORD: u32 = 0x800;
const SPELL: u32 = 0x801;
const ROCK: u32 = 0x802;
const TREE: u32 = 0x803;

fn master() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    p.push_top(Record::new(KYWD, KEYWORD).zsub(tag::EDID, "MyKeyword"));
    p.push_top(Record::new(SPEL, SPELL).zsub(tag::EDID, "MySpell"));
    p.push_top(Record::new(tag::STAT, ROCK).zsub(tag::EDID, "Rock"));
    p.push_top(Record::new(tag::STAT, TREE).zsub(tag::EDID, "Tree"));
    p
}

fn order() -> LoadOrder {
    let mut order = LoadOrder::new();
    let bytes = master().to_bytes();
    order
        .add("Master.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    order
}

#[test]
fn a_form_id_and_file_pair_resolves_to_the_form() {
    let order = order();
    let id = FormRef::parse("0x800~Master.esm").resolve(&order).unwrap();
    assert_eq!(order.record(order.winner_ref(id).unwrap()).tag, KYWD);
}

#[test]
fn an_editor_id_resolves_by_searching_the_winners() {
    let order = order();
    let by_edid = FormRef::parse("MySpell").resolve(&order).unwrap();
    let by_pair = FormRef::parse("0x801~Master.esm").resolve(&order).unwrap();
    assert_eq!(by_edid, by_pair);
}

#[test]
fn a_reference_into_an_absent_plugin_resolves_to_nothing() {
    let order = order();
    assert!(FormRef::parse("0x800~NotHere.esp")
        .resolve(&order)
        .is_none());
    assert!(FormRef::parse("NoSuchEditorId").resolve(&order).is_none());
}

#[test]
fn a_file_reference_matches_every_form_the_file_owns() {
    let order = order();
    let file = FormRef::parse("Master.esm");
    let rock = FormRef::parse("Rock").resolve(&order).unwrap();
    assert!(file.matches(&order, rock));
    // It names a set, so it is not one form.
    assert!(file.resolve(&order).is_none());
}

#[test]
fn a_keyword_file_resolves_to_an_add_keyword_patch() {
    let order = order();
    let file = KidFile::from_ini(&Ini::parse(
        "t_KID.ini",
        "Keyword = MyKeyword|Armor|Iron\nKeyword = Missing|Armor|Iron\n",
    ));
    assert_eq!(file.rules.len(), 2);

    let patches = resolve_kid(&file, &order);
    // The second rule names a keyword that is not there, so it drops.
    assert_eq!(patches.len(), 1);
    let keyword = FormRef::parse("MyKeyword").resolve(&order).unwrap();
    assert_eq!(patches[0].change, Change::AddKeyword { keyword });
    assert_eq!(patches[0].line, 1);
}

#[test]
fn a_distribution_resolves_to_what_it_hands_out() {
    let order = order();
    let file = DistrFile::from_ini(&Ini::parse(
        "t_DISTR.ini",
        "Spell = MySpell|Bandit||5/20|F|1|50\nDeathItem = Missing\n",
    ));
    let patches = resolve_spid(&file, &order);
    assert_eq!(patches.len(), 1);
    let spell = FormRef::parse("MySpell").resolve(&order).unwrap();
    match &patches[0].change {
        Change::Distribute { what, on_death, .. } => {
            assert_eq!(*what, spell);
            assert!(!on_death);
        }
        other => panic!("expected a distribution, got {other:?}"),
    }
    assert!((patches[0].chance - 50.0).abs() < f32::EPSILON);
}

#[test]
fn a_swap_resolves_both_sides_and_names_its_target() {
    let order = order();
    let file = SwapFile::from_ini(&Ini::parse("t_SWAP.ini", "[Forms]\nRock|Tree\n"));
    let patches = resolve_bos(&file, &order);
    assert_eq!(patches.len(), 1);
    let rock = FormRef::parse("Rock").resolve(&order).unwrap();
    let tree = FormRef::parse("Tree").resolve(&order).unwrap();
    assert_eq!(patches[0].target, Some(rock));
    assert_eq!(patches[0].change, Change::SwapBase { to: tree });
}

#[test]
fn a_swap_whose_replacement_is_absent_keeps_its_properties_only() {
    let order = order();
    let file = SwapFile::from_ini(&Ini::parse(
        "t_SWAP.ini",
        "[Forms]\nRock|0x999~Gone.esp|scale(2)\n",
    ));
    let patches = resolve_bos(&file, &order);
    assert_eq!(patches.len(), 1);
    assert_eq!(
        patches[0].change,
        Change::SetProperties {
            properties: "scale(2)".to_string()
        }
    );
}

#[test]
fn a_swap_set_on_the_left_produces_one_patch_per_form() {
    let order = order();
    let file = SwapFile::from_ini(&Ini::parse("t_SWAP.ini", "[Forms]\nRock,Tree|MySpell\n"));
    let patches = resolve_bos(&file, &order);
    assert_eq!(patches.len(), 2);
}

#[test]
fn a_transforms_section_never_swaps_a_base() {
    let order = order();
    let file = SwapFile::from_ini(&Ini::parse("t_SWAP.ini", "[Transforms]\nRock|scale(1.5)\n"));
    let patches = resolve_bos(&file, &order);
    assert_eq!(
        patches[0].change,
        Change::SetProperties {
            properties: "scale(1.5)".to_string()
        }
    );
}

#[test]
fn a_patch_carries_where_it_came_from() {
    let order = order();
    let file = KidFile::from_ini(&Ini::parse(
        "MyMod_KID.ini",
        "; a comment\nKeyword = MyKeyword|Armor|Iron\n",
    ));
    let patches = resolve_kid(&file, &order);
    assert_eq!(patches[0].source.to_str(), Some("MyMod_KID.ini"));
    // The comment occupies line 1, so the rule is on line 2.
    assert_eq!(patches[0].line, 2);
}

#[test]
fn resolution_preserves_deterministic_chance_instead_of_erasing_it() {
    let file = DistrFile::from_ini(&Ini::parse(
        "t_DISTR.ini",
        "Spell = MySpell|NONE|NONE|NONE|NONE|1|50!",
    ));
    let patches = resolve_spid(&file, &order());
    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].chance, 50.0);
    assert!(matches!(
        patches[0].change,
        Change::Distribute {
            deterministic_chance: true,
            ..
        }
    ));
}
