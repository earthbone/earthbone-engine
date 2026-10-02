// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
use soe_papyrus::Value;
use soe_papyrus::asm::{FunctionBuilder, ScriptBuilder, ident, s};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_SCRIPT: AtomicUsize = AtomicUsize::new(0);

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in 1..=3 {
        let mut data = vec![0; 244];
        data[4..8].copy_from_slice(&10.0f32.to_le_bytes());
        plugin.push_top(Record::new(Tag::new(b"IMAD"), object).sub(Tag::new(b"DNAM"), data));
    }
    plugin.push_top(
        Record::new(Tag::new(b"KYWD"), 10).sub(Tag::new(b"EDID"), b"VendorItemFood\0".to_vec()),
    );
    let mut order = LoadOrder::new();
    order.add("Contracts.esm", plugin).unwrap();
    Game::new(order)
}

fn invoke(
    game: &mut Game,
    target: Option<GlobalFormId>,
    class: &str,
    name: &str,
    args: &[Value],
) -> Value {
    let script = format!("Scr63Probe{}", NEXT_SCRIPT.fetch_add(1, Ordering::Relaxed));
    let mut function = FunctionBuilder::new("Run", "Var")
        .global()
        .local("result", "Var");
    let mut operands = if target.is_some() {
        function = function.param("target", "Var");
        vec![s(name), ident("target"), ident("result")]
    } else {
        vec![s(class), s(name), ident("result")]
    };
    for index in 0..args.len() {
        let param = format!("arg{index}");
        function = function.param(&param, "Var");
        operands.push(ident(&param));
    }
    let opcode = if target.is_some() {
        CallMethod
    } else {
        CallStatic
    };
    let bytes = ScriptBuilder::new(&script, "")
        .function(
            function
                .ins(opcode, operands)
                .ins(Return, vec![ident("result")])
                .build(),
        )
        .bytes();
    game.load_script(&bytes).unwrap();
    let mut values = target
        .map(|form| Value::Object(game.object_of_form(form)))
        .into_iter()
        .collect::<Vec<_>>();
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    game.vm.thread_result(thread).unwrap_or(Value::None)
}

#[test]
fn scr_63_binding_inventory_matches_the_ck_contracts() {
    let game = game();
    let methods = [
        ("ImageSpaceModifier", "Apply"),
        ("ImageSpaceModifier", "ApplyCrossFade"),
        ("ImageSpaceModifier", "PopTo"),
        ("ImageSpaceModifier", "Remove"),
        ("Keyword", "GetString"),
        ("Keyword", "SendStoryEvent"),
        ("Keyword", "SendStoryEventAndWait"),
    ];
    let globals = [("ImageSpaceModifier", "RemoveCrossFade")];

    for (class, name) in methods {
        assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
        assert!(
            !game.vm.natives.has_global(class, name),
            "{class}.{name} must be a method"
        );
    }
    for (class, name) in globals {
        assert!(game.vm.natives.has_global(class, name), "{class}.{name}");
        assert!(
            !game.vm.natives.has_method(class, name),
            "{class}.{name} must be global"
        );
    }
}

#[test]
fn scr_63_image_space_and_keyword_calls_follow_documented_contracts() {
    let mut game = game();
    let modifier = |game: &mut Game, object| Value::Object(game.object_of_form(id(object)));

    invoke(
        &mut game,
        Some(id(1)),
        "ImageSpaceModifier",
        "Apply",
        &[Value::Float(1.75)],
    );
    invoke(
        &mut game,
        Some(id(2)),
        "ImageSpaceModifier",
        "ApplyCrossFade",
        &[Value::Float(2.0)],
    );
    game.tick(1.0);
    assert_eq!(
        game.world()
            .image_space_layers()
            .iter()
            .map(|layer| (layer.modifier.object, layer.strength))
            .collect::<Vec<_>>(),
        vec![(1, 1.75), (2, 0.5)]
    );
    let destination = modifier(&mut game, 3);
    invoke(
        &mut game,
        Some(id(1)),
        "ImageSpaceModifier",
        "PopTo",
        &[destination, Value::Float(2.25)],
    );
    assert_eq!(
        game.world()
            .image_space_layers()
            .iter()
            .map(|layer| (layer.modifier.object, layer.strength))
            .collect::<Vec<_>>(),
        vec![(3, 2.25), (2, 0.5)]
    );
    invoke(&mut game, Some(id(3)), "ImageSpaceModifier", "Remove", &[]);
    assert_eq!(game.world().image_space_layers()[0].modifier, id(2));
    invoke(
        &mut game,
        None,
        "ImageSpaceModifier",
        "RemoveCrossFade",
        &[Value::Float(0.0)],
    );
    assert_eq!(game.world().image_space_layers(), &[]);

    invoke(&mut game, Some(id(1)), "ImageSpaceModifier", "Apply", &[]);
    let default_destination = modifier(&mut game, 3);
    invoke(
        &mut game,
        Some(id(1)),
        "ImageSpaceModifier",
        "PopTo",
        &[default_destination],
    );
    assert_eq!(
        game.world().image_space_layers()[0].strength,
        1.0,
        "Apply and PopTo default strengths are 1.0"
    );
    invoke(
        &mut game,
        Some(id(2)),
        "ImageSpaceModifier",
        "ApplyCrossFade",
        &[],
    );
    game.tick(0.5);
    assert_eq!(game.world().image_space_layers()[1].strength, 0.5);
    invoke(
        &mut game,
        None,
        "ImageSpaceModifier",
        "RemoveCrossFade",
        &[],
    );
    game.tick(0.5);
    assert_eq!(game.world().image_space_layers()[1].strength, 0.25);
    game.tick(0.5);
    assert_eq!(game.world().image_space_layers().len(), 1);

    let keyword = modifier(&mut game, 10);
    assert_eq!(
        invoke(&mut game, Some(id(10)), "Keyword", "GetString", &[],),
        Value::string("VendorItemFood")
    );
    let before = game.world().state_hash();
    assert_eq!(
        invoke(
            &mut game,
            Some(id(10)),
            "Keyword",
            "SendStoryEvent",
            &[
                Value::None,
                Value::None,
                Value::None,
                Value::Int(7),
                Value::Int(9),
            ],
        ),
        Value::None
    );
    assert_eq!(
        invoke(
            &mut game,
            Some(id(10)),
            "Keyword",
            "SendStoryEventAndWait",
            &[
                Value::None,
                Value::None,
                Value::None,
                Value::Int(7),
                Value::Int(9),
            ],
        ),
        Value::Bool(false)
    );
    assert_eq!(game.world().state_hash(), before);
    assert!(keyword.as_object().is_some());
}
