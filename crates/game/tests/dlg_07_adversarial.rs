// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass on DLG-07 (persuasion and intimidation CTDA functions).
//!
//! Source for the expected formula: `docs/game.md`, DLG-07 section, added in
//! this same change. It states the persuasion pair reads "the subject's
//! Speechcraft (current for the plain number, current plus permanent for the
//! total)" and that the total exists "so a perk or effect bonus is felt".
//! `crates/game/src/actor_values.rs` documents `modify` as the permanent
//! (perk-shaped) component and `modify_temporary` as "an effect's
//! contribution to the temporary component".
//!
//! An earlier draft of docs/game.md described GetTotalPersuasionNumber (315)
//! as `current() + permanent()`, and an earlier draft of the implementation
//! computed `base() + permanent()`, dropping both the temporary (effect)
//! component and one whole copy of the documented `+ permanent` term. Neither
//! TES5Edit nor the CK wiki documents an encoding for either function
//! (Open Question 1 in the task pack), so the current docs/game.md instead
//! states that 225 and 315 both return the subject's plain `current()` with
//! no separate `+ permanent` term, "since no source separates the plain
//! number from the total". The implementation matches that. The first test
//! below still isolates the effect-bonus half of the original claim, which
//! the current formula also satisfies (`current()` already includes the
//! temporary component). The second test is updated to the current,
//! documented formula: it no longer expects the extra `+ permanent` term.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const OWNER: u32 = 0x800;
const SPEAKER: u32 = 0x900;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER));
    plugin.push_top(Record::new(Tag(*b"ACHR"), SPEAKER).sub(Tag(*b"NAME"), OWNER.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Dlg07.esm", plugin).unwrap();
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
            owner: id(OWNER),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
        },
        c,
    )
}

/// Isolates the "effect" half of docs/game.md's claim: a purely temporary
/// Speechcraft bonus (a Fortify Speechcraft spell or potion, which
/// `actor_values.rs` documents as going through `modify_temporary`) must
/// change GetTotalPersuasionNumber (315) once it changes GetPersuasionNumber
/// (225), per "current plus permanent for the total" and "so ... an effect
/// bonus is felt". No permanent (perk) component is involved, so this cannot
/// be blamed on the separate `+ permanent` double count.
#[test]
fn dlg_07_total_persuasion_number_ignores_a_pure_effect_bonus_adversarial() {
    let game = fixture();
    game.world_mut()
        .set_actor_value(id(SPEAKER), "speechcraft", 20.0)
        .unwrap();
    game.world_mut()
        .mod_temporary_actor_value(id(SPEAKER), "speechcraft", 25.0)
        .unwrap();

    // GetPersuasionNumber (225) is documented and implemented as `current()`,
    // which is base(20) + permanent(0) + temporary(25) + damage(0) = 45.
    let plain = condition(225, 45.0);
    assert_eq!(
        evaluate(&game, id(SPEAKER), &plain),
        Ok(true),
        "sanity: GetPersuasionNumber should read the current Speechcraft including the active effect"
    );

    // docs/game.md: total = current + permanent = 45 + 0 = 45 here, since
    // permanent is untouched. The temporary Fortify effect must still be felt.
    let total = condition(315, 45.0);
    assert_eq!(
        evaluate(&game, id(SPEAKER), &total),
        Ok(true),
        "GetTotalPersuasionNumber must include the active Fortify Speechcraft effect \
         (docs/game.md: \"so a perk or effect bonus is felt\"); the implementation \
         reads base() + permanent() and returns 20.0 instead, silently dropping the effect"
    );
}

/// Isolates the "perk" half: with a permanent (perk-shaped) bonus applied
/// through `modify` and no temporary component at all, docs/game.md now
/// states that GetTotalPersuasionNumber (315) returns the same `current()`
/// value as GetPersuasionNumber (225), with no separate `+ permanent` term
/// added on top (no source documents an encoding that would justify counting
/// the permanent component twice).
#[test]
fn dlg_07_total_persuasion_number_equals_current_with_a_permanent_bonus_adversarial() {
    let game = fixture();
    game.world_mut()
        .set_actor_value(id(SPEAKER), "speechcraft", 20.0)
        .unwrap();
    game.world_mut()
        .mod_actor_value(id(SPEAKER), "speechcraft", 15.0)
        .unwrap();

    // current() = base(20) + permanent(15) = 35. docs/game.md: the total
    // equals current(), 35, with no extra + permanent term.
    let total = condition(315, 35.0);
    assert_eq!(
        evaluate(&game, id(SPEAKER), &total),
        Ok(true),
        "GetTotalPersuasionNumber must equal current() = 35 per docs/game.md \
         (no source separates the plain number from the total, so neither adds \
         an extra + permanent term)"
    );
}
