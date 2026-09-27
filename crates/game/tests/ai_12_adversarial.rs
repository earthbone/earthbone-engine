// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};

const Q: u32 = 0x800;
const BASE: u32 = 0x801;
const OTHER: u32 = 0x802;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(form: u32) -> Vec<u8> {
    let mut data = vec![0, 0, 0, 0];
    data.extend_from_slice(&1.0f32.to_bits().to_le_bytes());
    data.extend_from_slice(&72u16.to_le_bytes());
    data.extend_from_slice(&[0; 2]);
    data.extend_from_slice(&form.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data
}

fn build_game(alias_flags: u32, references: &[(u32, u32, bool)]) -> Game {
    let alias = quest::build_record(Q, "AliasSearch", 0)
        .sub(Tag(*b"ALST"), 0u32.to_le_bytes())
        .zsub(Tag(*b"ALID"), "Candidate")
        .sub(Tag(*b"FNAM"), alias_flags.to_le_bytes())
        .sub(Tag(*b"CTDA"), condition(BASE))
        .sub(Tag(*b"ALED"), Vec::new());
    let mut plugin = Plugin::new(FileHeader { flags: flags::MASTER, ..Default::default() });
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"STAT"), OTHER));
    for (object, base, disabled) in references {
        let flags = if *disabled { flags::INITIALLY_DISABLED } else { 0 };
        plugin.push_top(Record::new(Tag(*b"REFR"), *object)
            .with_flags(flags)
            .sub(Tag(*b"NAME"), base.to_le_bytes()));
    }
    plugin.push_top(alias);
    let mut order = LoadOrder::new();
    order.add("AliasSearch.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_12_disabled_candidates_require_the_allow_disabled_flag() {
    let mut game = build_game(quest::alias_flags::OPTIONAL, &[(0x900, BASE, true)]);
    assert!(game.start_quest(id(Q)));
    assert_eq!(game.world_mut().alias_ref(id(Q), 0), None);

    let mut game = build_game(
        quest::alias_flags::OPTIONAL | quest::alias_flags::ALLOW_DISABLED,
        &[(0x900, BASE, true)],
    );
    assert!(game.start_quest(id(Q)));
    assert_eq!(game.world_mut().alias_ref(id(Q), 0), Some(id(0x900)));
}

#[test]
fn ai_12_search_accepts_authored_achr_candidates_as_well_as_refr() {
    let mut plugin = Plugin::new(FileHeader { flags: flags::MASTER, ..Default::default() });
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(
        quest::build_record(Q, "AliasSearch", 0)
            .sub(Tag(*b"ALST"), 0u32.to_le_bytes())
            .zsub(Tag(*b"ALID"), "Candidate")
            .sub(Tag(*b"FNAM"), quest::alias_flags::OPTIONAL.to_le_bytes())
            .sub(Tag(*b"CTDA"), condition(BASE))
            .sub(Tag(*b"ALED"), Vec::new()),
    );
    let mut order = LoadOrder::new();
    order.add("AliasSearch.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.start_quest(id(Q)));
    assert_eq!(game.world_mut().alias_ref(id(Q), 0), Some(id(0x900)));
}
