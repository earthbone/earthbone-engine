// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::ai;
use soe_game::npc::{self, Config, Stats};
use soe_game::package::{self, Schedule};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x800;
const LIST: u32 = 0x801;
const FIRST: u32 = 0x802;
const SECOND: u32 = 0x803;
const CELL: u32 = 0x810;
const ACTOR: u32 = 0x811;

fn game() -> Game {
    let mut p = Plugin::new(FileHeader::default());
    p.push_top(npc::build_record(BASE, "GuardBase", "Guard", Config::default(), Stats::default()));
    p.push_top(package::build_record(FIRST, "First", 0, Schedule::default()));
    p.push_top(package::build_record(SECOND, "Second", 0, Schedule::default()));
    p.push_top(Record::new(T(b"FLST"), LIST).zsub(T(b"EDID"), "PackageList")
        .sub(T(b"LNAM"), FIRST.to_le_bytes()).sub(T(b"LNAM"), SECOND.to_le_bytes()));
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
    order.add("MOD10.esm", p).unwrap();
    Game::new(order)
}

#[test]
fn mod_10_package_formlist_routes_its_members_into_the_default_package_stack() {
    let mut game = game();
    let root = std::env::temp_dir().join(format!("soe-mod10-adversarial-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Test_DISTR.ini"), "Package = PackageList|GuardBase\n").unwrap();
    let report = game.apply_mod_data(std::slice::from_ref(&root));
    assert_eq!(report.distributions_applied, 1);
    let candidates = ai::candidates(&mut game.world_mut(), soe_esm::GlobalFormId { file: 0, object: ACTOR });
    let forms = candidates.iter().map(|c| c.package).collect::<Vec<_>>();
    assert_eq!(forms, vec![
        soe_esm::GlobalFormId { file: 0, object: FIRST },
        soe_esm::GlobalFormId { file: 0, object: SECOND },
    ]);
    let _ = std::fs::remove_dir_all(root);
}
