// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass on DLG-07, malformed-parameter angle.
//!
//! `docs/game.md`'s DLG-07 section and the task pack both cite
//! `wbDefinitionsTES5.pas` and `Raw_Function_Data.wikitext` for the claim that
//! IsIntimidatedbyPlayer (116), GetPersuasionNumber (225),
//! GetTotalPersuasionNumber (315) and GetIntimidateSuccess (655) "take no
//! parameter". `crates/game/src/conditions.rs` establishes its own convention
//! for exactly this situation elsewhere in the same dispatcher: function 70
//! (`if raw > 1 { return Err(Error::Parameter(...)) }`, line 293) and function
//! 448 (`if c.parameters[1] != 0 { return Err(Error::Parameter(...)) }`, line
//! 409) both reject a CTDA record whose parameter slot carries a value the
//! function does not use, so a corrupted or hand-edited INFO record cannot be
//! silently misread. The four DLG-07 match arms (lines 314-348, 454-477) read
//! `subject` only and never inspect `c.parameters` at all, so a CTDA record
//! with garbage in a slot the function is documented not to use evaluates
//! exactly as if that slot were zero, instead of surfacing `Error::Parameter`
//! the way the dispatcher's own established pattern requires.

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
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Dlg07Malformed.esm", plugin).unwrap();
    Game::new(order)
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

/// GetPersuasionNumber (225), GetTotalPersuasionNumber (315) and
/// IsIntimidatedbyPlayer (116) are documented as taking no parameter, but the
/// dispatcher's own established convention (function 70 and function 448, see
/// module docs) is to reject a nonzero value in a slot the function does not
/// use. A hand-edited or corrupted INFO record with garbage in that slot
/// should fail closed the same way. It
/// must not evaluate as though the slot were legitimately zero.
#[test]
fn dlg_07_persuasion_functions_reject_a_garbage_parameter_like_the_established_pattern_adversarial()
{
    let game = fixture();
    game.world_mut()
        .set_actor_value(id(0x900), "speechcraft", 40.0)
        .unwrap();

    for function in [116, 225, 315] {
        let garbage = Condition {
            flags: 0,
            comparison: Comparison::Number(40.0),
            function,
            parameters: [0xDEAD_BEEF, 0],
            run_on: 0,
            reference: 0,
            parameter_3: -1,
            strings: [None, None],
        };
        let result = evaluate(&game, id(0x900), &garbage);
        assert!(
            matches!(result, Err(conditions::Error::Parameter(_, _))),
            "function {function} should reject a nonzero, no-parameter slot the \
             way function 70 and function 448 do (conditions.rs line 293, \
             line 409), got {result:?} instead of Error::Parameter"
        );
    }
}

/// Same gap for GetIntimidateSuccess (655), the fourth documented
/// no-parameter function in the DLG-07 set (docs/game.md, conditions.rs line
/// 454). The formula only reads the player's Speechcraft and the subject's
/// Confidence, so a garbage parameter changes nothing about the outcome
/// instead of failing the way `Error::Parameter` would for a used slot.
#[test]
fn dlg_07_intimidate_success_reject_a_garbage_parameter_like_the_established_pattern_adversarial() {
    let game = fixture();
    let player = game.world_mut().player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 60.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(id(0x900), "confidence", 2.0)
        .unwrap();

    let garbage = Condition {
        flags: 0,
        comparison: Comparison::Number(1.0),
        function: 655,
        parameters: [0x1234_5678, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    let result = evaluate(&game, id(0x900), &garbage);
    assert!(
        matches!(result, Err(conditions::Error::Parameter(_, _))),
        "GetIntimidateSuccess (655) should reject a nonzero, no-parameter slot \
         the way function 70 and function 448 do (conditions.rs line 293, \
         line 409), got {result:?} instead of Error::Parameter"
    );
}
