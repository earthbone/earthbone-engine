// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use std::{fs, path::PathBuf};
const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ROCK: u32 = 0x800;
const TREE: u32 = 0x801;
const CELL: u32 = 0x802;
const REF: u32 = 0x803;
fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}
struct Dir(PathBuf);
impl Dir {
    fn new(case: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("mod03-adversarial-{}-{case}", std::process::id()));
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
    p.push_top(Record::new(T(b"STAT"), ROCK).zsub(T(b"EDID"), "Rock"));
    p.push_top(Record::new(T(b"STAT"), TREE).zsub(T(b"EDID"), "Tree"));
    let cell = p.push_record(
        Record::new(T(b"CELL"), CELL)
            .zsub(T(b"EDID"), "Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let rec = Record::new(T(b"REFR"), REF)
        .sub(T(b"NAME"), ROCK.to_le_bytes())
        .sub(
            T(b"DATA"),
            [1f32, 2., 3., 0., 0., 0.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>(),
        )
        .sub(T(b"XSCL"), 2f32.to_le_bytes());
    let ri = p.push_record(rec);
    let mut temp = Group::new(GroupKind::CellTemporary(CELL));
    temp.entries.push(Entry::Record(ri));
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
    let mut o = LoadOrder::new();
    o.add("Base.esm", Plugin::parse(&p.to_bytes()).unwrap())
        .unwrap();
    Game::new(o)
}
#[test]
fn mod_03_relative_transform_scale_and_flag_clear_apply_to_authored_state() {
    let d = Dir::new("relative");
    fs::write(d.0.join("Props_SWAP.ini"),"[Forms]\nRock|Tree\n[Properties]\nRock|posR(10,20,30),rotR(0,0,90),scale(3),flags(0x20),flagsC(0x10)|Chance(100)\n").unwrap();
    let mut g = game();
    g.apply_mod_data(&d.roots());
    let s = g.world_mut().reference(id(REF)).unwrap().clone();
    assert_eq!(s.position, [11., 22., 33.]);
    assert!((s.rotation[2] - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
    assert_eq!(s.scale, 6.);
    assert_eq!(s.record_flags & 0x30, 0x20);
}
#[test]
fn mod_03_references_section_swaps_the_named_reference() {
    let d = Dir::new("precedence");
    fs::write(
        d.0.join("Props_SWAP.ini"),
        "[Properties]\nRock|pos(10,10,10)\n0x803|pos(7,8,9)\n[References]\n0x803|Tree\n",
    )
    .unwrap();
    let mut g = game();
    g.apply_mod_data(&d.roots());
    let s = g.world_mut().reference(id(REF)).unwrap().clone();
    assert_eq!(
        s.position,
        [7., 8., 9.],
        "reference-specific properties take precedence over original-base properties"
    );
    assert_eq!(s.base, Some(id(TREE)));
}
