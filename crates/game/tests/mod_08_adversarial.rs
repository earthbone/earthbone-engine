// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::npc::{self, Config, Stats};
use soe_game::Game;
use std::collections::BTreeSet;
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const SHOUT: u32 = 0x801;
const CELL: u32 = 0x810;
const ACTOR: u32 = 0x811;

fn id(object: u32) -> GlobalFormId { GlobalFormId { file: 0, object } }

fn fixture() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(npc::build_record(BASE, "GuardBase", "Guard", Config::default(), Stats::default()));
    p.push_top(Record::new(T(b"SHOU"), SHOUT).zsub(T(b"EDID"), "TestShout"));
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
    order.add("MOD08.esm", p).unwrap();
    Game::new(order)
}

struct Dir(PathBuf);
impl Dir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("soe-mod08-adversarial-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("Test_DISTR.ini"),
            "Shout = TestShout|GuardBase\nShout = TestShout|GuardBase\n").unwrap();
        Self(path)
    }
    fn roots(&self) -> Vec<PathBuf> { vec![self.0.clone()] }
}
impl Drop for Dir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[test]
fn mod_08_reapplying_duplicate_shout_rules_keeps_one_learned_shout_across_save_load() {
    let dir = Dir::new();
    let mut game = fixture();
    game.apply_mod_data(&dir.roots());
    game.world_mut().actor(id(ACTOR)).unwrap();
    game.apply_mod_data(&dir.roots());
    let expected = BTreeSet::from([id(SHOUT)]);
    assert_eq!(game.world().actor_ref(id(ACTOR)).unwrap().shouts, expected);
    let saved = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&saved).unwrap();
    assert_eq!(restored.world().actor_ref(id(ACTOR)).unwrap().shouts, expected);
}
