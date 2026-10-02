// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::records::gameplay::{Comparison, Condition};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, conditions};

#[test]
fn ai_86_both_unavailable_contracts_report_diagnostics_for_all_comparison_shapes() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    let mut order = LoadOrder::new();
    order.add("ConditionContracts.esm", plugin).unwrap();
    let game = Game::new(order);
    for function in [409, 530] {
        for comparison in [Comparison::Number(0.0), Comparison::Number(1.0)] {
            let condition = Condition {
                flags: 0,
                comparison,
                function,
                parameters: [0, 0],
                run_on: 0,
                reference: 0,
                parameter_3: -1,
                strings: [None, None],
            };
            let result = conditions::evaluate_one(
                &mut game.world_mut(),
                &game.vm,
                conditions::Context {
                    owner: GlobalFormId {
                        file: 0,
                        object: 0x800,
                    },
                    subject: GlobalFormId {
                        file: 0,
                        object: 0x800,
                    },
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
                &condition,
            );
            assert_eq!(result, Err(conditions::Error::Function(function)));
        }
    }
}
