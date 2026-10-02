// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, LoadOrder, Plugin, Record, Tag, plugin::flags};
use soe_game::Game;
use soe_game::npc::{self, Config, Stats};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_CASE: AtomicUsize = AtomicUsize::new(0);

const EDID: Tag = Tag::new(b"EDID");
const NPC: u32 = 0x800;
const SPELL: u32 = 0x801;

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(Tag::new(b"SPEL"), SPELL).zsub(EDID, "FilterSpell"));
    plugin.push_top(npc::build_record(
        NPC,
        "FilterActor",
        "Filter Actor",
        Config {
            flags: npc::flags::FEMALE | npc::flags::UNIQUE | npc::flags::SUMMONABLE,
            level: 15,
            ..Config::default()
        },
        Stats {
            skills: [50; 18],
            ..Stats::default()
        },
    ));
    let mut order = LoadOrder::new();
    order
        .add("Filters.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

fn apply(line: &str) -> soe_game::moddata::ModDataReport {
    let root = std::env::temp_dir().join(format!(
        "soe-mod05-{}-{}",
        std::process::id(),
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Filters_DISTR.ini"), line).unwrap();
    let mut game = game();
    let report = game.apply_mod_data(std::slice::from_ref(&root));
    std::fs::remove_dir_all(root).unwrap();
    report
}

#[test]
fn mod_05_range_endpoints_are_inclusive_and_all_filters_must_match() {
    let report = apply("Spell = FilterSpell|FilterActor||15/15,0(50/50)|F/U/S\n");
    assert_eq!(report.distributions_applied, 1);
    assert_eq!(report.skipped_unsupported_filter, 0);
}

#[test]
fn mod_05_a_failed_skill_filter_rejects_the_rule_without_marking_it_unsupported() {
    let report = apply("Spell = FilterSpell|FilterActor||15/15,0(51/60)|F/U/S\n");
    assert_eq!(report.distributions_applied, 0);
    assert_eq!(report.skipped_unsupported_filter, 0);
}

#[test]
fn mod_05_starts_dead_remains_skipped_without_actor_state() {
    let report = apply("Spell = FilterSpell|FilterActor||15/15|F/U/S/-D\n");
    assert_eq!(report.distributions_applied, 0);
    assert_eq!(report.skipped_unsupported_filter, 1);
}
