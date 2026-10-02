// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

#[test]
fn ai_67_integer_graph_variable_condition_is_supported() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Ai67Adversarial.esm", plugin).unwrap();
    let game = Game::new(order);
    let mut world = game.world_mut();
    let actor = world
        .actor(GlobalFormId {
            file: 0,
            object: 0x900,
        })
        .unwrap();
    actor.graph_ints.insert("isstaggering".into(), 1);
    let condition = |expected| Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function: 675,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [Some("IsStaggering".into()), None],
    };
    assert!(conditions::supports_function(675));
    assert_eq!(
        conditions::evaluate_one(
            &mut world,
            &game.vm,
            conditions::Context {
                owner: GlobalFormId {
                    file: 0,
                    object: 0x800
                },
                subject: GlobalFormId {
                    file: 0,
                    object: 0x900
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
            &condition(1.0)
        ),
        Ok(true)
    );
    assert_eq!(
        conditions::evaluate_one(
            &mut world,
            &game.vm,
            conditions::Context {
                owner: GlobalFormId {
                    file: 0,
                    object: 0x800
                },
                subject: GlobalFormId {
                    file: 0,
                    object: 0x900
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
            &condition(0.0)
        ),
        Ok(false)
    );
    let missing_name = Condition {
        strings: [None, None],
        ..condition(1.0)
    };
    assert_eq!(
        conditions::evaluate_one(
            &mut world,
            &game.vm,
            conditions::Context {
                owner: GlobalFormId {
                    file: 0,
                    object: 0x800
                },
                subject: GlobalFormId {
                    file: 0,
                    object: 0x900
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
            &missing_name
        ),
        Err(conditions::Error::Variable("missing CIS1".into()))
    );
    assert_eq!(
        conditions::evaluate_one(
            &mut world,
            &game.vm,
            conditions::Context {
                owner: GlobalFormId {
                    file: 0,
                    object: 0x800
                },
                subject: GlobalFormId {
                    file: 0,
                    object: 0x900
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
            &Condition {
                run_on: 1,
                ..condition(1.0)
            }
        ),
        Err(conditions::Error::Target(1))
    );
}
