// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/installed.rs"]
mod installed;
#[path = "common/legacy.rs"]
mod legacy;

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

#[test]
fn animated_expiry_and_crossfade_pause_with_the_game_clock() {
    let mut game = game();
    call(&mut game, Some(5), "Apply", &[]);
    call(&mut game, Some(1), "ApplyCrossFade", &[]);
    game.tick_menu_mode(20.0, true);
    assert_eq!(weights(&game), vec![(5, 1.0), (1, 0.0)]);
    assert!(game
        .world()
        .image_space_layers()
        .iter()
        .all(|layer| layer.age == 0.0));
    game.tick(1.0);
    assert_eq!(weights(&game), vec![(5, 1.0), (1, 1.0)]);
    game.tick(1.0);
    assert_eq!(weights(&game), vec![(1, 1.0)]);
}

#[test]
fn invalid_apply_and_pop_leave_the_existing_stack_unchanged() {
    let game = game();
    game.world_mut().apply_image_space(id(1), 1.0).unwrap();
    let before = game.world().save_state();
    assert!(game.world_mut().pop_image_space(id(1), id(6), 1.0).is_err());
    assert!(game.world_mut().apply_image_space(id(2), f32::NAN).is_err());
    assert!(game
        .world_mut()
        .crossfade_image_space(None, f32::INFINITY)
        .is_err());
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn corrupt_fade_load_is_atomic_and_an_older_save_starts_without_modifiers() {
    let mut game = game();
    let mut legacy = game.world().save_state();
    legacy::truncate_to_version(&mut legacy, 24, |candidate| {
        self::game().world_mut().load_state(candidate).is_ok()
    });
    game.world_mut()
        .crossfade_image_space(Some(id(1)), 0.0)
        .unwrap();
    game.world_mut()
        .crossfade_image_space(Some(id(2)), 2.0)
        .unwrap();
    game.tick(1.0);
    let before = game.world().save_state();
    let mut corrupt = before.clone();
    let mut marker = vec![1];
    marker.extend_from_slice(&1.0_f64.to_le_bytes());
    marker.extend_from_slice(&2.0_f64.to_le_bytes());
    marker.push(1);
    marker.extend_from_slice(&id(2).file.to_le_bytes());
    marker.extend_from_slice(&id(2).object.to_le_bytes());
    let offset = legacy::unique_offset(&corrupt, &marker) + 1 + std::mem::size_of::<f64>();
    corrupt[offset..offset + std::mem::size_of::<f64>()].copy_from_slice(&f64::NAN.to_le_bytes());
    assert!(game
        .world_mut()
        .load_state(&corrupt)
        .unwrap_err()
        .to_string()
        .contains("invalid image-space fade"));
    assert_eq!(game.world().save_state(), before);
    game.world_mut().load_state(&legacy).unwrap();
    assert!(game.world().image_space_layers().is_empty());
}

#[test]
fn installed_imad_lifetimes_and_curves_sample_winning_records() {
    let Ok(_path) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let ids: Vec<_> = order
        .winners_of(Tag::new(b"IMAD"))
        .filter(|(_, record)| record.flags & 0x20 == 0)
        .map(|(id, _)| id)
        .collect();
    assert!(
        !ids.is_empty(),
        "installed masters must contain IMAD records"
    );
    let mut game = Game::from_shared_order(order);
    for &id in &ids {
        game.world_mut().apply_image_space(id, 1.0).unwrap();
    }
    assert_eq!(game.world().image_space_layers().len(), ids.len());
    for layer in game.world().image_space_layers() {
        let time = game.world().image_space_layer_time(&layer).unwrap();
        game.world()
            .image_space_curve_samples(layer.modifier, time)
            .unwrap();
    }
    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    eprintln!("IMAD winning lifetimes accepted: {}", ids.len());
}

#[test]
fn mag_71_samples_authored_color_blur_depth_and_hdr_curves() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(keyed_record(1));
    let mut order = LoadOrder::new();
    order.add("ImageCurves.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().apply_image_space(id(1), 0.25).unwrap();
    game.tick(1.0);
    let layer = game.world().image_space_layers().remove(0);
    assert_eq!(layer.strength, 0.25);
    let time = game.world().image_space_layer_time(&layer).unwrap();
    assert_eq!(time, 0.5);
    let samples = game.world().image_space_curve_samples(id(1), time).unwrap();
    let find = |channel: [u8; 4]| {
        samples
            .iter()
            .find(|sample| sample.channel == channel)
            .unwrap()
            .values
            .clone()
    };
    let close = |actual: Vec<f32>, expected: &[f32]| {
        assert_eq!(actual.len(), expected.len());
        assert!(actual
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() < 0.000_001));
    };
    close(find(*b"TNAM"), &[0.5, 0.25, 0.125, 0.5]);
    close(find(*b"BNAM"), &[0.5]);
    close(find(*b"WNAM"), &[0.5]);
    close(find(*b"\x13IAD"), &[2.0]);
    close(find(*b"\x53IAD"), &[2.0]);
}
