// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! MOD-08: SPID distributed shouts enter the actor's learned shout state and
//! survive the whole-game save format.

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::npc::{self, Config, Stats};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR_BASE: u32 = 0x800;
const SHOUT: u32 = 0x801;
const CELL: u32 = 0x810;
const ACTOR: u32 = 0x811;

fn id(object: u32) -> soe_esm::GlobalFormId {
    soe_esm::GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(npc::build_record(
        ACTOR_BASE,
        "WhiterunGuard",
        "Whiterun Guard",
        Config::default(),
        Stats::default(),
    ));
    plugin.push_top(Record::new(T(b"SHOU"), SHOUT).zsub(T(b"EDID"), "TestShout"));

    let cell = plugin.push_record(
        Record::new(T(b"CELL"), CELL)
            .zsub(T(b"EDID"), "TestCell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let actor = plugin.push_record(
        Record::new(T(b"ACHR"), ACTOR)
            .zsub(T(b"EDID"), "TestGuard")
            .sub(T(b"NAME"), ACTOR_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0u8; 24]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(actor));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut subblock = Group::new(GroupKind::InteriorSubBlock(0));
    subblock.entries.push(Entry::Record(cell));
    subblock.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(subblock));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);

    let mut order = LoadOrder::new();
    order.add("MOD08.esm", plugin).unwrap();
    Game::new(order)
}

struct DistributionDir(PathBuf);

impl DistributionDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "soe-mod-08-{}-{}",
            std::process::id(),
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary directory is created");
        std::fs::write(
            path.join("Test_DISTR.ini"),
            "Shout = TestShout|WhiterunGuard\nShout = TestShout|WhiterunGuard\n",
        )
        .expect("distribution fixture is written");
        Self(path)
    }

    fn roots(&self) -> Vec<PathBuf> {
        vec![self.0.clone()]
    }
}

impl Drop for DistributionDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn mod_08_distributed_shout_reaches_learned_state_once() {
    let directory = DistributionDir::new();
    let mut game = fixture();
    let report = game.apply_mod_data(&directory.roots());
    assert_eq!(report.distributions_applied, 2);

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).expect("fixture actor exists");
    assert_eq!(actor.shouts, BTreeSet::from([id(SHOUT)]));
    assert!(!actor.spells.contains(&id(SHOUT)));
}

#[test]
fn mod_08_distributed_shout_persists_through_engine_save_load() {
    let directory = DistributionDir::new();
    let mut game = fixture();
    game.apply_mod_data(&directory.roots());
    game.world_mut()
        .actor(id(ACTOR))
        .expect("fixture actor exists");
    let saved = game.save_state().expect("whole-game save serializes");

    let mut restored = fixture();
    restored
        .load_state(&saved)
        .expect("whole-game save loads against its original load order");
    assert_eq!(
        restored.world().actor_ref(id(ACTOR)).unwrap().shouts,
        BTreeSet::from([id(SHOUT)]),
        "the learned shout survives the engine save/load path"
    );
}
