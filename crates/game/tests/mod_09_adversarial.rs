// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const RACE: u32 = 0x801;
const SKIN_A: u32 = 0x802;
const SKIN_B: u32 = 0x803;
const ADDON: u32 = 0x804;
const CELL: u32 = 0x810;
const ACTOR: u32 = 0x811;

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

fn fixture() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(Record::new(T(b"RACE"), RACE).zsub(T(b"EDID"), "TestRace"));
    for (form, name) in [(SKIN_A, "SkinA"), (SKIN_B, "SkinB")] {
        p.push_top(Record::new(T(b"ARMO"), form).zsub(T(b"EDID"), name).sub(T(b"MODL"), ADDON.to_le_bytes()));
    }
    p.push_top(Record::new(T(b"ARMA"), ADDON)
        .sub(T(b"BOD2"), [1_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat())
        .sub(T(b"DNAM"), [0_u8; 12]).sub(T(b"RNAM"), RACE.to_le_bytes()));
    p.push_top(npc::build_record(BASE, "GuardBase", "Guard", Config::default(), Stats::default())
        .sub(T(b"RNAM"), RACE.to_le_bytes()));
    let cell = p.push_record(Record::new(T(b"CELL"), CELL).zsub(T(b"EDID"), "Cell").sub(T(b"DATA"), [1u8, 0]));
    let actor = p.push_record(Record::new(T(b"ACHR"), ACTOR).zsub(T(b"EDID"), "Guard")
        .sub(T(b"NAME"), BASE.to_le_bytes()).sub(T(b"DATA"), vec![0; 24]));
    let mut temp = Group::new(GroupKind::CellTemporary(CELL));
    temp.entries.push(Entry::Record(actor));
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
    let mut order = LoadOrder::new();
    order.add("MOD09.esm", p).unwrap();
    Game::new(order)
}

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("soe-mod09-adversarial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("Test_DISTR.ini"),
            "Skin = SkinA|GuardBase\nSkin = SkinB|GuardBase\n").unwrap();
        Self(path)
    }
    fn roots(&self) -> Vec<PathBuf> { vec![self.0.clone()] }
}
impl Drop for Dir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[test]
fn mod_09_last_skin_rule_selects_its_resolved_armor_and_addon_identity() {
    let dir = Dir::new();
    let mut game = fixture();
    let report = game.apply_mod_data(&dir.roots());
    assert_eq!(report.distributions_applied, 2);
    let appearance = game.world_mut().actor_appearance(id(ACTOR)).unwrap();
    assert_eq!(appearance.skin, Some(id(SKIN_B)));
    assert_eq!(appearance.parts[0].armor, id(SKIN_B));
    assert_eq!(appearance.parts[0].addon, id(ADDON));
}
