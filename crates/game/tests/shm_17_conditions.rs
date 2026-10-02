// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::conditions::{
    ComparisonValue, ConditionContext, ConditionError, TESCondition, TESConditionItem,
    CONDITIONITEMOBJECT, CONDITION_ITEM_DATA,
};
use soe_commonlib_shim::{form::FormId, refr::ObjectRefHandle};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn shm_17_tes_condition_reads_live_actor_values_through_the_mit_api() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("ConditionBridge.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 37.0)
        .unwrap();

    let condition = TESCondition {
        items: vec![TESConditionItem {
            data: CONDITION_ITEM_DATA {
                flags: 0,
                comparison: ComparisonValue::Number(37.0),
                function: 14,
                parameters: [24, 0],
                run_on: 0,
                reference: FormId(0),
                parameter_3: -1,
                object: CONDITIONITEMOBJECT::SelfRef,
            },
        }],
    };
    let context = ConditionContext {
        owner: FormId(game.world().runtime_id(id(0x800))),
        subject: ObjectRefHandle(game.world().runtime_id(id(0x900))),
        target: None,
        quest: None,
    };
    assert!(condition.is_true(&mut game, context).unwrap());

    let unsupported = TESCondition {
        items: vec![TESConditionItem {
            data: CONDITION_ITEM_DATA {
                flags: 0,
                comparison: ComparisonValue::Number(0.0),
                function: 65535,
                parameters: [0, 0],
                run_on: 0,
                reference: FormId(0),
                parameter_3: -1,
                object: CONDITIONITEMOBJECT::SelfRef,
            },
        }],
    };
    assert_eq!(
        unsupported.is_true(&mut game, context).unwrap_err(),
        ConditionError::UnsupportedFunction(65535)
    );
}
