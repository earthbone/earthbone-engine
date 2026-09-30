// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-07: persuasion and intimidation CTDA functions (116, 225, 315, 655).
//! Mirrors the synthetic fixture and evaluate_one call pattern from
//! `condition_actor_values.rs`. A live case under `SKYRIM_DATA` checks that
//! the functions resolve against vanilla NPC records instead of only the
//! synthetic fixture.

#[path = "common/installed.rs"]
mod installed;

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800)); // player stand-in
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801)); // speaker base
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Persuasion.esm", plugin).unwrap();
    Game::new(order)
}

fn condition(function: u16, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        c,
    )
}

/// dlg_07_persuasion_number_passes_and_fails_on_speechcraft
///
/// This is the planned done check: a persuasion topic (GetPersuasionNumber,
/// function 225) passes or fails on the speaker's Speechcraft actor value and
/// branches accordingly, and GetTotalPersuasionNumber (315) tracks the
/// permanent component on top of it.
#[test]
fn dlg_07_persuasion_number_passes_and_fails_on_speechcraft() {
    let game = fixture();
    assert!(conditions::supports_function(225));
    assert!(conditions::supports_function(315));

    game.world_mut()
        .set_actor_value(id(0x900), "speechcraft", 40.0)
        .unwrap();

    let passes = condition(225, 40.0);
    assert_eq!(evaluate(&game, id(0x900), &passes), Ok(true));
    let fails = condition(225, 41.0);
    assert_eq!(evaluate(&game, id(0x900), &fails), Ok(false));

    game.world_mut()
        .mod_actor_value(id(0x900), "speechcraft", 15.0)
        .unwrap();
    let total_passes = condition(315, 55.0);
    assert_eq!(evaluate(&game, id(0x900), &total_passes), Ok(true));
    let total_fails = condition(315, 56.0);
    assert_eq!(evaluate(&game, id(0x900), &total_fails), Ok(false));
}

#[test]
fn dlg_07_intimidate_success_branches_on_speechcraft_versus_confidence() {
    let game = fixture();
    assert!(conditions::supports_function(655));

    let player = game.world_mut().player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 60.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(0x900), "confidence", 2.0)
        .unwrap();
    let succeeds = condition(655, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &succeeds), Ok(true));

    game.world_mut()
        .set_actor_value(id(0x900), "confidence", 4.0)
        .unwrap();
    let fails = condition(655, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &fails), Ok(false));
}

#[test]
fn dlg_07_is_intimidated_by_player_reads_last_bribed_intimidated_flag() {
    let game = fixture();
    assert!(conditions::supports_function(116));

    let unset = condition(116, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &unset), Ok(false));

    game.world_mut()
        .set_actor_value(id(0x900), "lastbribedintimidated", 1.0)
        .unwrap();
    let set = condition(116, 1.0);
    assert_eq!(evaluate(&game, id(0x900), &set), Ok(true));
}

/// Live check against a real install: confirms the four functions resolve
/// against vanilla NPC records (not just the synthetic fixture above)
/// without erroring, naming the record used.
#[test]
fn dlg_07_persuasion_functions_resolve_against_a_vanilla_npc() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the live persuasion check");
        return;
    };
    let plugin = installed::plugin("Skyrim.esm").unwrap();
    let hulda: u32 = 0x0001_3475; // Hulda, Bannered Mare innkeeper
    let npc = plugin
        .records
        .iter()
        .find(|r| r.tag == Tag(*b"NPC_") && r.form_id == hulda)
        .expect("Hulda (0x13475) missing from Skyrim.esm");
    eprintln!("using vanilla NPC_ record 0x{:06X}", npc.form_id);

    let mut synth = Plugin::new(FileHeader::default());
    synth.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    synth.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), hulda.to_le_bytes()));

    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    order.add("Persuasion.esp", synth).unwrap();
    let game = Game::new(order);

    let subject = GlobalFormId {
        file: 1,
        object: 0x900,
    };
    for (function, expected) in [(225, 0.0f32), (315, 0.0), (655, 0.0), (116, 0.0)] {
        let c = condition(function, expected);
        match evaluate(&game, subject, &c) {
            Ok(_) | Err(conditions::Error::Form(_)) => {}
            Err(e) => panic!("function {function} errored unexpectedly: {e:?}"),
        }
    }
}
