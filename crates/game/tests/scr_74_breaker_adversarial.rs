// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use soe_esm::{
    plugin::flags, Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record,
    Tag,
};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, Return};
use soe_papyrus::Value;

const TAG: fn(&[u8; 4]) -> Tag = Tag::new;
const PLAYER: u32 = 0x14;
const RECEIVER: u32 = 0x15;
const PLAYER_BASE: u32 = 0xA00;
const CELL: u32 = 0xA01;
const CLOTHES: u32 = 0xA02;
const OTHER_CLOTHES: u32 = 0xA03;
static PROBE: AtomicUsize = AtomicUsize::new(0);

fn le(id: u32) -> [u8; 4] {
    id.to_le_bytes()
}

fn armor(id: u32, name: &str) -> Record {
    let mut body = le(1 << 2).to_vec();
    body.extend_from_slice(&le(2));
    Record::new(TAG(b"ARMO"), id)
        .zsub(TAG(b"EDID"), name)
        .sub(TAG(b"DNAM"), le(0))
        .sub(TAG(b"BOD2"), body)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "test".into(),
        ..Default::default()
    });
    plugin.push_top(
        Record::new(TAG(b"NPC_"), PLAYER_BASE)
            .zsub(TAG(b"EDID"), "PlayerBase")
            .sub(TAG(b"ACBS"), [0; 24])
            .sub(TAG(b"AIDT"), [0; 20]),
    );
    plugin.push_top(armor(CLOTHES, "TestClothes"));
    plugin.push_top(armor(OTHER_CLOTHES, "OtherClothes"));
    let cell_record = plugin.push_record(
        Record::new(TAG(b"CELL"), CELL)
            .zsub(TAG(b"EDID"), "TestCell")
            .sub(TAG(b"DATA"), [1, 0]),
    );
    let player_record = plugin.push_record(
        Record::new(TAG(b"ACHR"), PLAYER)
            .zsub(TAG(b"EDID"), "PlayerRef")
            .sub(TAG(b"NAME"), le(PLAYER_BASE))
            .sub(TAG(b"DATA"), [0; 24]),
    );
    let receiver_record = plugin.push_record(
        Record::new(TAG(b"ACHR"), RECEIVER)
            .zsub(TAG(b"EDID"), "ReceiverRef")
            .sub(TAG(b"NAME"), le(PLAYER_BASE))
            .sub(TAG(b"DATA"), [0; 24]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(player_record));
    temporary.entries.push(Entry::Record(receiver_record));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell_record));
    block.entries.push(Entry::Group(children));
    plugin.groups.push(block);

    let plugin = Plugin::parse(&plugin.to_bytes()).unwrap();
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.bind();
    game.load_cell(GlobalFormId {
        file: 0,
        object: CELL,
    });
    game
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Str(_) => "String",
        _ => "Form",
    }
}

fn call(game: &mut Game, target_id: u32, method: &str, args: &[Value]) {
    let id = PROBE.fetch_add(1, Ordering::Relaxed);
    let script = format!("Scr74BreakerProbe{id}");
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .param("target", "Form")
        .local("result", "Var");
    let mut operands = vec![s(method), ident("target"), ident("result")];
    for (index, arg) in args.iter().enumerate() {
        let name = format!("a{index}");
        function = function.param(&name, type_name(arg));
        operands.push(ident(&name));
    }
    let function = function
        .ins(CallMethod, operands)
        .ins(Return, vec![ident("result")])
        .build();
    game.load_script(
        &ScriptBuilder::new(&script, "Form")
            .function(function)
            .bytes(),
    )
    .unwrap();
    let mut values = vec![Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: target_id,
    }))];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    game.run_until_idle(64);
    let _ = game.vm.thread_result(thread);
}

fn form(game: &mut Game, id: u32) -> Value {
    Value::Object(game.object_of_form(GlobalFormId {
        file: 0,
        object: id,
    }))
}

#[test]
fn equip_defaults_give_missing_item_and_allow_later_unequip() {
    let mut game = game();
    let item = GlobalFormId {
        file: 0,
        object: CLOTHES,
    };
    let item_value = form(&mut game, CLOTHES);
    call(&mut game, PLAYER, "EquipItem", &[item_value]);
    {
        let mut world = game.world_mut();
        let actor = world
            .actor(GlobalFormId {
                file: 0,
                object: PLAYER,
            })
            .unwrap();
        assert!(actor.equipped.contains(&item));
        assert!(!actor.prevent_equipment_removal.contains(&item));
    }
    assert_eq!(
        game.world_mut().item_count(
            GlobalFormId {
                file: 0,
                object: PLAYER
            },
            item
        ),
        1
    );
    assert_eq!(game.world().notifications, ["Equipped TestClothes"]);

    let item_value = form(&mut game, CLOTHES);
    call(&mut game, PLAYER, "UnequipItem", &[item_value]);
    let mut world = game.world_mut();
    let actor = world
        .actor(GlobalFormId {
            file: 0,
            object: PLAYER,
        })
        .unwrap();
    assert!(!actor.equipped.contains(&item));
    assert!(!actor.prevent_equipment_removal.contains(&item));
}

#[test]
fn repeated_equipping_does_not_repeat_message_or_drop_protection() {
    let mut game = game();
    let item = form(&mut game, CLOTHES);
    call(
        &mut game,
        PLAYER,
        "EquipItem",
        &[item.clone(), Value::Bool(true)],
    );
    call(
        &mut game,
        PLAYER,
        "EquipItem",
        &[item, Value::Bool(false), Value::Bool(false)],
    );
    assert_eq!(game.world().notifications, ["Equipped TestClothes"]);
    {
        let mut world = game.world_mut();
        let actor = world
            .actor(GlobalFormId {
                file: 0,
                object: PLAYER,
            })
            .unwrap();
        assert!(actor.prevent_equipment_removal.contains(&GlobalFormId {
            file: 0,
            object: CLOTHES
        }));
    }
    let item_value = form(&mut game, CLOTHES);
    call(&mut game, PLAYER, "UnequipItem", &[item_value]);
    assert!(game
        .world_mut()
        .actor(GlobalFormId {
            file: 0,
            object: PLAYER
        })
        .unwrap()
        .equipped
        .contains(&GlobalFormId {
            file: 0,
            object: CLOTHES
        }));
}

#[test]
fn missing_form_is_a_quiet_no_op() {
    let mut game = game();
    call(&mut game, PLAYER, "EquipItem", &[Value::None]);
    assert!(game.world().notifications.is_empty());
    let mut world = game.world_mut();
    let actor = world
        .actor(GlobalFormId {
            file: 0,
            object: PLAYER,
        })
        .unwrap();
    assert!(actor.equipped.is_empty());
    assert!(actor.prevent_equipment_removal.is_empty());
}

#[test]
fn protected_equipment_can_still_move_to_another_reference() {
    let mut game = game();
    let player = GlobalFormId {
        file: 0,
        object: PLAYER,
    };
    let receiver = GlobalFormId {
        file: 0,
        object: RECEIVER,
    };
    let item = GlobalFormId {
        file: 0,
        object: CLOTHES,
    };
    let item_value = form(&mut game, CLOTHES);
    let receiver_value = form(&mut game, RECEIVER);
    call(
        &mut game,
        PLAYER,
        "EquipItem",
        &[item_value.clone(), Value::Bool(true)],
    );
    call(
        &mut game,
        PLAYER,
        "RemoveItem",
        &[item_value, Value::Int(1), Value::Bool(true), receiver_value],
    );

    assert_eq!(game.world_mut().item_count(player, item), 0);
    assert_eq!(game.world_mut().item_count(receiver, item), 1);
    let mut world = game.world_mut();
    let actor = world.actor(player).unwrap();
    assert!(!actor.equipped.contains(&item));
    assert!(!actor.prevent_equipment_removal.contains(&item));
}

#[test]
fn silent_equip_applies_item_without_queuing_notification() {
    let mut game = game();
    let item_value = form(&mut game, CLOTHES);
    call(
        &mut game,
        PLAYER,
        "EquipItem",
        &[item_value, Value::Bool(false), Value::Bool(true)],
    );
    assert!(game.world().notifications.is_empty());
    assert!(game
        .world_mut()
        .actor(GlobalFormId {
            file: 0,
            object: PLAYER
        })
        .unwrap()
        .equipped
        .contains(&GlobalFormId {
            file: 0,
            object: CLOTHES
        }));
}

#[test]
fn saved_protection_blocks_replacement_and_bulk_removal_clears_it() {
    let mut source = game();
    let protected = GlobalFormId {
        file: 0,
        object: CLOTHES,
    };
    let protected_value = form(&mut source, CLOTHES);
    call(
        &mut source,
        PLAYER,
        "EquipItem",
        &[protected_value, Value::Bool(true)],
    );
    let bytes = source.save_state().unwrap();

    let mut restored = game();
    restored.load_state(&bytes).unwrap();
    let replacement_value = form(&mut restored, OTHER_CLOTHES);
    call(
        &mut restored,
        PLAYER,
        "EquipItem",
        &[replacement_value, Value::Bool(false)],
    );
    {
        let mut world = restored.world_mut();
        let actor = world
            .actor(GlobalFormId {
                file: 0,
                object: PLAYER,
            })
            .unwrap();
        assert!(actor.equipped.contains(&protected));
        assert!(actor.prevent_equipment_removal.contains(&protected));
        assert!(!actor.equipped.contains(&GlobalFormId {
            file: 0,
            object: OTHER_CLOTHES
        }));
    }

    call(&mut restored, PLAYER, "UnequipAll", &[]);
    {
        let mut world = restored.world_mut();
        let actor = world
            .actor(GlobalFormId {
                file: 0,
                object: PLAYER,
            })
            .unwrap();
        assert!(!actor.equipped.contains(&protected));
        assert!(!actor.prevent_equipment_removal.contains(&protected));
    }

    let protected_value = form(&mut restored, CLOTHES);
    call(
        &mut restored,
        PLAYER,
        "EquipItem",
        &[protected_value, Value::Bool(true)],
    );
    call(
        &mut restored,
        PLAYER,
        "RemoveAllItems",
        &[Value::None, Value::Bool(false)],
    );
    let mut world = restored.world_mut();
    let actor = world
        .actor(GlobalFormId {
            file: 0,
            object: PLAYER,
        })
        .unwrap();
    assert!(!actor.equipped.contains(&protected));
    assert!(!actor.prevent_equipment_removal.contains(&protected));
    assert_eq!(
        world.item_count(
            GlobalFormId {
                file: 0,
                object: PLAYER
            },
            protected
        ),
        0
    );
}
