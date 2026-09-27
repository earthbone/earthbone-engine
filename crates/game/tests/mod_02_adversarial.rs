// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use std::{fs, path::PathBuf};
const T: fn(&[u8; 4]) -> Tag = Tag::new;
const K1: u32 = 0x800;
const K2: u32 = 0x801;
const K3: u32 = 0x802;
const ARM: u32 = 0x803;
fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}
struct Dir(PathBuf);
impl Dir {
    fn new(case: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("mod02-adversarial-{}-{case}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn roots(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn game() -> Game {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    for (n, name) in [(K1, "One"), (K2, "Two"), (K3, "Three")] {
        p.push_top(Record::new(T(b"KYWD"), n).zsub(T(b"EDID"), name));
    }
    p.push_top(Record::new(T(b"ARMO"), ARM).zsub(T(b"EDID"), "TestArmor"));
    let mut o = LoadOrder::new();
    o.add("Base.esm", Plugin::parse(&p.to_bytes()).unwrap())
        .unwrap();
    Game::new(o)
}
#[test]
fn mod_02_overlapping_groups_exclude_members_across_the_group_union() {
    let d = Dir::new("overlap");
    fs::write(d.0.join("A_KID.ini"),"ExclusiveGroup = A|One,Two\nExclusiveGroup = B|Two,Three\nKeyword = Two|Armor|TestArmor\nKeyword = One|Armor|TestArmor\n").unwrap();
    let mut g = game();
    let r = g.apply_mod_data(&d.roots());
    let kws = g.world().keywords(id(ARM));
    assert_eq!(
        r.keywords_attached, 1,
        "a later rule cannot add a keyword exclusive with one already attached"
    );
    assert_eq!(
        kws.iter()
            .filter(|k| [id(K1), id(K2), id(K3)].contains(k))
            .count(),
        1
    );
}
#[test]
fn mod_02_invalid_group_forms_do_not_make_non_keyword_forms_exclusive() {
    let d = Dir::new("invalid");
    fs::write(
        d.0.join("Bad_KID.ini"),
        "ExclusiveGroup = G|TestArmor,One\nKeyword = One|Armor|TestArmor\n",
    )
    .unwrap();
    let mut g = game();
    let report = g.apply_mod_data(&d.roots());
    assert!(
        g.world().keywords(id(ARM)).contains(&id(K1)),
        "only valid KYWD records define KID exclusivity, report {report:?}"
    );
}
