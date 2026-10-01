// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, CallStatic, Return};
use soe_papyrus::Value;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn record(object: u32, animated: bool, duration: f32) -> Record {
    let mut data = vec![0; 244];
    data[..4].copy_from_slice(&u32::from(animated).to_le_bytes());
    data[4..8].copy_from_slice(&duration.to_le_bytes());
    Record::new(Tag::new(b"IMAD"), object).sub(Tag::new(b"DNAM"), data)
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for object in 1..=4 {
        plugin.push_top(record(object, false, 0.0));
    }
    plugin.push_top(record(5, true, 2.0));
    plugin.push_top(Record::new(Tag::new(b"IMAD"), 6).sub(Tag::new(b"DNAM"), vec![0; 8]));
    let mut order = LoadOrder::new();
    order.add("Images.esm", plugin).unwrap();
    Game::new(order)
}

fn keyed_record(object: u32) -> Record {
    let mut header = vec![0; 244];
    header[..4].copy_from_slice(&1u32.to_le_bytes());
    header[4..8].copy_from_slice(&2.0f32.to_le_bytes());
    for offset in [176, 180, 212] {
        header[offset..offset + 4].copy_from_slice(&2u32.to_le_bytes());
    }
    let mult_count = 8 + 19 * 8;
    let add_count = mult_count + 4;
    header[mult_count..mult_count + 4].copy_from_slice(&2u32.to_le_bytes());
    header[add_count..add_count + 4].copy_from_slice(&2u32.to_le_bytes());
    let curve = |values: &[(f32, &[f32])]| {
        let mut data = Vec::new();
        for (time, components) in values {
            data.extend_from_slice(&time.to_le_bytes());
            for component in *components {
                data.extend_from_slice(&component.to_le_bytes());
            }
        }
        data
    };
    let mut record = Record::new(Tag::new(b"IMAD"), object).sub(Tag::new(b"DNAM"), header);
    for (tag, values) in [
        (
            *b"TNAM",
            vec![
                (0.0, vec![0.0, 0.0, 0.0, 0.0]),
                (1.0, vec![1.0, 0.5, 0.25, 1.0]),
            ],
        ),
        (*b"BNAM", vec![(0.0, vec![0.2]), (1.0, vec![0.8])]),
        (*b"WNAM", vec![(0.0, vec![0.1]), (1.0, vec![0.9])]),
        (*b"\x13IAD", vec![(0.0, vec![1.0]), (1.0, vec![3.0])]),
        (*b"\x53IAD", vec![(0.0, vec![0.0]), (1.0, vec![4.0])]),
    ] {
        let data = curve(
            &values
                .iter()
                .map(|(time, components)| (*time, components.as_slice()))
                .collect::<Vec<_>>(),
        );
        record = record.sub(Tag::new(&tag), data);
    }
    record
}

fn call(game: &mut Game, target: Option<u32>, name: &str, args: &[Value]) {
    let script_name = format!("ImageCall{}", game.vm.time().to_bits());
    let mut function = FunctionBuilder::new("Run", "None")
        .global()
        .param("target", "ImageSpaceModifier");
    let mut operands = if target.is_some() {
        vec![s(name), ident("target"), ident("unused")]
    } else {
        vec![s("ImageSpaceModifier"), s(name), ident("unused")]
    };
    for index in 0..args.len() {
        let arg = format!("arg{index}");
        function = function.param(&arg, "Var");
        operands.push(ident(&arg));
    }
    let script = ScriptBuilder::new(&script_name, "")
        .function(
            function
                .ins(
                    if target.is_some() {
                        CallMethod
                    } else {
                        CallStatic
                    },
                    operands,
                )
                .ins(Return, vec![soe_papyrus::asm::none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let mut values = vec![target.map_or(Value::None, |object| {
        Value::Object(game.object_of_form(id(object)))
    })];
    values.extend_from_slice(args);
    let thread = game.vm.call_global(&script_name, "Run", &values).unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn weights(game: &Game) -> Vec<(u32, f32)> {
    game.world()
        .image_space_layers()
        .into_iter()
        .map(|layer| (layer.modifier.object, layer.strength))
        .collect()
}

#[test]
fn scr_33_apply_dispatch_uses_default_and_unclamped_strength() {
    let mut game = game();
    call(&mut game, Some(1), "Apply", &[]);
    call(&mut game, Some(2), "Apply", &[Value::Float(2.5)]);

    assert_eq!(weights(&game), vec![(1, 1.0), (2, 2.5)]);
}

#[test]
fn scr_33_popto_dispatch_changes_source_without_interrupting_other_fade() {
    let mut game = game();
    call(&mut game, Some(1), "Apply", &[Value::Float(2.0)]);
    call(&mut game, Some(3), "ApplyCrossFade", &[Value::Float(2.0)]);
    game.tick(1.0);
    let destination = Value::Object(game.object_of_form(id(4)));
    call(
        &mut game,
        Some(1),
        "PopTo",
        &[destination, Value::Float(3.0)],
    );

    assert_eq!(weights(&game), vec![(4, 3.0), (3, 0.5)]);
    game.tick(1.0);
    assert_eq!(weights(&game), vec![(4, 3.0), (3, 1.0)]);
}

#[test]
fn scr_33_remove_and_crossfade_dispatches_remove_their_own_layers() {
    let mut game = game();
    call(&mut game, Some(1), "Apply", &[]);
    call(&mut game, Some(2), "ApplyCrossFade", &[]);
    game.tick(0.5);
    assert_eq!(weights(&game), vec![(1, 1.0), (2, 0.5)]);

    call(&mut game, Some(1), "Remove", &[]);
    call(&mut game, None, "RemoveCrossFade", &[]);
    game.tick(0.5);
    assert_eq!(weights(&game), vec![(2, 0.25)]);
    game.tick(0.5);
    assert!(weights(&game).is_empty());
}

#[test]
fn scr_33_invalid_modifier_calls_log_errors_without_changing_layers() {
    let mut game = game();
    call(&mut game, Some(1), "Apply", &[]);
    let before = weights(&game);

    call(&mut game, Some(6), "Apply", &[Value::Float(2.0)]);
    let destination = Value::Object(game.object_of_form(id(6)));
    call(
        &mut game,
        Some(1),
        "PopTo",
        &[destination, Value::Float(2.0)],
    );
    call(&mut game, Some(6), "ApplyCrossFade", &[Value::Float(2.0)]);
    call(
        &mut game,
        None,
        "RemoveCrossFade",
        &[Value::Float(f32::INFINITY)],
    );

    assert_eq!(weights(&game), before);
    assert!(game.vm.log.contains("invalid IMAD data size"));
    assert!(game.vm.log.contains("non-finite image-space fade duration"));
}

#[test]
fn natives_preserve_unclamped_strength_and_unrelated_crossfade() {
    let mut game = game();
    call(&mut game, Some(1), "Apply", &[Value::Float(2.5)]);
    call(&mut game, Some(2), "Apply", &[Value::Float(-0.5)]);
    call(&mut game, Some(3), "ApplyCrossFade", &[Value::Float(2.0)]);
    game.tick(1.0);
    assert_eq!(weights(&game), vec![(1, 2.5), (2, -0.5), (3, 0.5)]);
    let next = Value::Object(game.object_of_form(id(4)));
    call(&mut game, Some(1), "PopTo", &[next, Value::Float(3.0)]);
    call(&mut game, Some(2), "Remove", &[]);
    assert_eq!(weights(&game), vec![(4, 3.0), (3, 0.5)]);
    call(&mut game, None, "RemoveCrossFade", &[Value::Float(1.0)]);
    game.tick(0.5);
    assert_eq!(weights(&game), vec![(4, 3.0), (3, 0.25)]);
    game.tick(0.5);
    assert_eq!(weights(&game), vec![(4, 3.0)]);
}

#[test]
fn interrupted_crossfade_resumes_from_current_weights_through_save_load() {
    let mut original = game();
    original
        .world_mut()
        .crossfade_image_space(Some(id(1)), 0.0)
        .unwrap();
    original
        .world_mut()
        .crossfade_image_space(Some(id(2)), 2.0)
        .unwrap();
    original.tick(1.0);
    original
        .world_mut()
        .crossfade_image_space(Some(id(3)), 4.0)
        .unwrap();
    assert_eq!(weights(&original), vec![(1, 0.5), (2, 0.5), (3, 0.0)]);
    original.tick(1.0);
    assert_eq!(weights(&original), vec![(1, 0.375), (2, 0.375), (3, 0.25)]);
    let saved = original.save_state().unwrap();
    let mut restored = game();
    restored.load_state(&saved).unwrap();
    assert_eq!(original.world().state_hash(), restored.world().state_hash());
    original.tick(3.0);
    restored.tick(3.0);
    assert_eq!(weights(&restored), vec![(3, 1.0)]);
    assert_eq!(original.world().save_state(), restored.world().save_state());
}
