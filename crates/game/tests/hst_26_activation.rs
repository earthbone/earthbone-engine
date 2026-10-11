// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-26 default pickup, ownership and CK Activate/OnActivate semantics.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::Field;
use soe_game::Game;
use soe_papyrus::asm::{b, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, IAdd, Return};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture(kind: [u8; 4]) -> (Game, GlobalFormId) {
    fixture_book_flags(kind, 0)
}

fn fixture_book_flags(kind: [u8; 4], flags: u8) -> (Game, GlobalFormId) {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 7)
            .zsub(b"EDID", "Player")
            .sub(b"ACBS", [0; 24]),
    );
    let mut data = vec![0; 16];
    data[0] = flags;
    let mut base = Record::new(Tag(kind), 42)
        .zsub(b"FULL", "Loose object")
        .sub(b"DATA", data);
    if kind == *b"FURN" {
        base = base
            .sub(b"MNAM", 1_u32.to_le_bytes())
            .sub(b"FNPR", [1, 0, 0, 0]);
    }
    plugin.push_top(base);
    plugin.push_top(Record::new(Tag(*b"FACT"), 44));
    plugin.push_top(
        Record::new(Tag(*b"CELL"), 45)
            .sub(b"DATA", [1, 0])
            .sub(b"XOWN", 44_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 43)
            .sub(b"NAME", 42_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 46)
            .sub(b"NAME", 7_u32.to_le_bytes())
            .sub(b"DATA", [0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Skyrim.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let probe = ScriptBuilder::new("Hst26Probe", "ObjectReference")
        .variable("count", "Int", i(0))
        .variable("lastActivator", "ObjectReference", none())
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("activator", "ObjectReference")
                .ins(IAdd, vec![ident("count"), ident("count"), i(1)])
                .ins(
                    soe_papyrus::Opcode::Assign,
                    vec![ident("lastActivator"), ident("activator")],
                )
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&probe).unwrap();
    let object = game.object_of_form(id(43));
    game.vm.attach(object, "Hst26Probe").unwrap();
    (game, id(43))
}

fn count(game: &mut Game, target: GlobalFormId) -> i32 {
    let object = game.object_of_form(target);
    game.vm
        .get_var(object, "Hst26Probe", "count")
        .unwrap()
        .as_int()
}

#[test]
fn hst_26_all_loose_item_types_pick_up_disable_and_run_onactivate_once() {
    for kind in [
        b"MISC", b"WEAP", b"ARMO", b"ALCH", b"BOOK", b"INGR", b"KEYM",
    ] {
        let (mut game, target) = fixture(*kind);
        let player = game.player();
        assert!(game.activate(target, player), "{kind:?}");
        assert_eq!(game.world_mut().item_count(player, id(42)), 1, "{kind:?}");
        assert!(
            !game.world().reference_ref(target).unwrap().enabled,
            "{kind:?}"
        );
        assert_eq!(count(&mut game, target), 1);
        let object = game.object_of_form(target);
        let activator = game.object_of_form(player);
        assert_eq!(
            game.vm.get_var(object, "Hst26Probe", "lastActivator"),
            Some(Value::Object(activator))
        );
        assert!(!game.activate(target, player), "already picked up {kind:?}");
        assert_eq!(game.world_mut().item_count(player, id(42)), 1);
    }
}

#[test]
fn hst_26_owned_pickup_preserves_stolen_owner_including_cell_inheritance() {
    for cell_owned in [false, true] {
        let (mut game, target) = fixture(*b"MISC");
        let player = game.player();
        let mut world = game.world_mut();
        let reference = world.reference(target).unwrap();
        if cell_owned {
            reference.cell = Some(id(45));
        } else {
            reference.owner = Some(id(44));
        }
        drop(world);
        assert!(game.world_mut().loose_item_is_stealing(target, player));
        assert!(game.activate(target, player));
        let world = game.world();
        let stack = &world
            .reference_ref(player)
            .unwrap()
            .inventory
            .stacks(id(42))[0];
        assert_eq!(
            stack.extra(33).unwrap().fields,
            vec![Field::Form(Some(id(44)))]
        );
    }
}

#[test]
fn hst_26_allowed_faction_pickup_is_not_stolen() {
    let (mut game, target) = fixture(*b"MISC");
    let player = game.player();
    let mut world = game.world_mut();
    world.actor(player).unwrap().factions.insert(id(44), 2);
    let reference = world.reference(target).unwrap();
    reference.owner = Some(id(44));
    reference.owner_rank = Some(2);
    drop(world);
    assert!(!game.world_mut().loose_item_is_stealing(target, player));
    assert!(game.activate(target, player));
    assert!(game
        .world()
        .reference_ref(player)
        .unwrap()
        .inventory
        .stacks(id(42))
        .iter()
        .all(|stack| stack.extra(33).is_none()));
}

#[test]
fn hst_26_activator_and_furniture_run_onactivate() {
    for kind in [b"ACTI", b"FURN"] {
        let (mut game, target) = fixture(*kind);
        let player = game.player();
        assert!(game.activate(target, player));
        assert_eq!(count(&mut game, target), 1);
        assert!(game.world().reference_ref(target).unwrap().enabled);
        assert_eq!(game.world_mut().item_count(player, id(42)), 0);
        if *kind == *b"FURN" {
            assert_eq!(game.world().furniture_seat(player), Some((target, 0)));
        }
    }
}

#[test]
fn hst_26_blocked_activation_sends_event_and_native_default_only_bypasses_block() {
    let (mut game, target) = fixture(*b"BOOK");
    let player = game.player();
    game.world_mut()
        .reference(target)
        .unwrap()
        .activation_blocked = true;
    assert!(!game.activate(target, player));
    assert_eq!(count(&mut game, target), 1);
    assert!(game.world().reference_ref(target).unwrap().enabled);
    assert_eq!(game.world_mut().item_count(player, id(42)), 0);
    assert!(game.world().menu_requests.is_empty());
    let native = ScriptBuilder::new("Hst26Native", "")
        .function(
            FunctionBuilder::new("Run", "Bool")
                .global()
                .param("target", "ObjectReference")
                .param("actor", "ObjectReference")
                .local("ok", "Bool")
                .ins(
                    CallMethod,
                    vec![
                        s("Activate"),
                        ident("target"),
                        ident("ok"),
                        ident("actor"),
                        b(true),
                    ],
                )
                .ins(Return, vec![ident("ok")])
                .build(),
        )
        .bytes();
    game.load_script(&native).unwrap();
    let object = game.object_of_form(target);
    let actor = game.object_of_form(player);
    let thread = game
        .vm
        .call_global(
            "Hst26Native",
            "Run",
            &[Value::Object(object), Value::Object(actor)],
        )
        .unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::Bool(true)));
    assert_eq!(count(&mut game, target), 1);
    assert_eq!(game.world_mut().item_count(player, id(42)), 1);
    assert_eq!(
        game.world().menu_requests,
        vec![soe_game::world::MenuRequest::ReadBook(id(42))]
    );
}

#[test]
fn hst_26_book_default_processing_requests_reading_only_for_the_player() {
    for player_activation in [false, true] {
        let (mut game, target) = fixture(*b"BOOK");
        let actor = if player_activation {
            game.player()
        } else {
            id(46)
        };
        assert!(game.activate(target, actor));
        assert_eq!(game.world_mut().item_count(actor, id(42)), 1);
        assert_eq!(
            game.world().menu_requests,
            if player_activation {
                vec![soe_game::world::MenuRequest::ReadBook(id(42))]
            } else {
                Vec::new()
            }
        );
        assert_eq!(count(&mut game, target), 1);
    }
}

#[test]
fn hst_26_inventory_overflow_leaves_reference_enabled_and_sends_onactivate() {
    let (mut game, target) = fixture(*b"MISC");
    let player = game.player();
    game.world_mut().add_item(player, id(42), i32::MAX);
    assert!(!game.activate(target, player));
    assert!(game.world().reference_ref(target).unwrap().enabled);
    assert_eq!(game.world_mut().item_count(player, id(42)), i32::MAX);
    assert_eq!(count(&mut game, target), 1);
}

#[test]
fn hst_26_untakeable_book_activates_without_removing_the_reference() {
    let (mut game, target) = fixture_book_flags(*b"BOOK", 2);
    let player = game.player();
    assert!(game.activate(target, player));
    assert!(game.world().reference_ref(target).unwrap().enabled);
    assert_eq!(game.world_mut().item_count(player, id(42)), 0);
    assert_eq!(count(&mut game, target), 1);
}
