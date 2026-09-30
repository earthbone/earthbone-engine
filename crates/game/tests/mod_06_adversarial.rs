// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const FIRST: u32 = 0x801;
const SECOND: u32 = 0x802;
const INSERTED: u32 = 0x803;
const CELL: u32 = 0x810;
const ACTOR: u32 = 0x811;

fn plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(npc::build_record(BASE, "GuardBase", "Guard", Config::default(), Stats::default())
        .sub(T(b"PKID"), FIRST.to_le_bytes())
        .sub(T(b"PKID"), SECOND.to_le_bytes()));
    for (id, name) in [(FIRST, "First"), (SECOND, "Second"), (INSERTED, "Inserted")] {
        p.push_top(package::build_record(id, name, 0, Schedule::default()));
    }
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
    p
}

#[test]
fn mod_06_package_rules_insert_at_the_requested_index_and_ignore_duplicate_forms() {
    let mut order = LoadOrder::new();
    order.add("MOD06.esm", plugin()).unwrap();
    let mut game = Game::new(order);
    let root = std::env::temp_dir().join(format!("soe-mod06-adversarial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Test_DISTR.ini"),
        "Package = Inserted|GuardBase||||1\nPackage = Inserted|GuardBase||||1\n").unwrap();
    game.apply_mod_data(std::slice::from_ref(&root));
    let candidates = ai::candidates(&mut game.world_mut(), soe_esm::GlobalFormId { file: 0, object: ACTOR });
    let forms = candidates.iter().map(|c| c.package).collect::<Vec<_>>();
    assert_eq!(forms, vec![
        soe_esm::GlobalFormId { file: 0, object: FIRST },
        soe_esm::GlobalFormId { file: 0, object: INSERTED },
        soe_esm::GlobalFormId { file: 0, object: SECOND },
    ]);
    let _ = std::fs::remove_dir_all(root);
}
