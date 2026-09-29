// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-37 DEST damage, stage models, events, clearing, and saved reference state.

use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn dstd(percent: u8, model_stage: u8, flags: u8) -> Vec<u8> {
    let mut data = vec![percent, 0, model_stage, flags];
    data.extend_from_slice(&0i32.to_le_bytes());
    data.extend_from_slice(&[0; 12]);
    data
}

fn plugin() -> Plugin {
    let dest = [100i32.to_le_bytes().as_slice(), &[2, 0, 0, 0]].concat();
    let base = Record::new(T(b"ACTI"), BASE)
        .sub(T(b"DEST"), dest)
        .sub(T(b"DSTD"), dstd(75, 1, 0))
        .sub(T(b"DMDL"), b"meshes/stage1.nif\0".to_vec())
        .sub(T(b"DSTF"), vec![])
        .sub(T(b"DSTD"), dstd(25, 1, 0b0110))
        .sub(T(b"DMDL"), b"meshes/stage2.nif\0".to_vec())
        .sub(T(b"DSTF"), vec![]);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base);
    plugin
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    order.add("Cmb37.esm", plugin()).unwrap();
    Game::new(order)
}

fn deal(game: &mut Game, reference: GlobalFormId, damage: f32) {
    let mut allocate = |_: &str, _: &str| ObjectId(1);
    game.world_mut()
        .damage_object(reference, damage, &mut allocate);
}

#[test]
fn cmb_37_damage_crosses_thresholds_emits_events_and_selects_stage_models() {
    let mut game = game();
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    assert_eq!(game.world_mut().destruction_stage(reference), -1);

    deal(&mut game, reference, 30.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 0);
    assert_eq!(
        game.world().destruction_model(reference).as_deref(),
        Some("meshes/stage1.nif")
    );
    assert!(matches!(
        game.world().pending.last(),
        Some(Pending::Event { name, args, .. })
            if name == "OnDestructionStageChanged" && args == &[soe_papyrus::Value::Int(0)]
    ));

    deal(&mut game, reference, 50.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 1);
    assert_eq!(
        game.world().destruction_model(reference).as_deref(),
        Some("meshes/stage2.nif")
    );
    let mut world = game.world_mut();
    let state = world.reference(reference).unwrap();
    assert!(state.destroyed && !state.enabled);
}

#[test]
fn cmb_37_clear_destruction_restores_health_stage_model_and_enable_state() {
    let mut game = game();
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    deal(&mut game, reference, 80.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 1);

    let mut allocate = |_: &str, _: &str| ObjectId(1);
    game.world_mut().clear_destruction(reference, &mut allocate);
    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(game.world().destruction_model(reference), None);
    let mut world = game.world_mut();
    let state = world.reference(reference).unwrap();
    assert!(!state.destroyed && state.enabled);
    drop(world);
    assert!(matches!(
        game.world().pending.last(),
        Some(Pending::Event { name, args, .. })
            if name == "OnDestructionStageChanged" && args == &[soe_papyrus::Value::Int(-1)]
    ));
}

#[test]
fn cmb_37_destruction_health_and_stage_survive_save_round_trip() {
    let mut runtime = game();
    let player = runtime.player();
    let reference = runtime.world_mut().place_at(id(BASE), player, false);
    deal(&mut runtime, reference, 30.0);
    let expected_model = runtime.world().destruction_model(reference);
    let save = runtime.world().save_state();

    let mut restored = game();
    restored.world_mut().load_state(&save).unwrap();
    assert_eq!(restored.world_mut().destruction_stage(reference), 0);
    assert_eq!(
        restored.world().destruction_model(reference),
        expected_model
    );
    deal(&mut restored, reference, 50.0);
    assert_eq!(restored.world_mut().destruction_stage(reference), 1);
}

#[test]
fn cmb_37_vanilla_destructible_accepts_damage_and_saves_its_stage() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping CMB-37 vanilla destructible survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut game = Game::new(order);
    let base = game
        .world()
        .order()
        .winners_of(T(b"ACTI"))
        .find_map(|(id, record)| {
            (!record.is_deleted()
                && record.find(T(b"DEST")).is_some()
                && record.all(T(b"DSTD")).next().is_some()
                && record.all(T(b"DMDL")).next().is_some())
            .then_some(id)
        })
        .expect("installed masters contain a modeled destructible ACTI");
    let player = game.player();
    let reference = game.world_mut().place_at(base, player, false);
    for _ in 0..16 {
        deal(&mut game, reference, 100_000.0);
    }
    assert!(game.world_mut().destruction_stage(reference) >= 0);
    let model = game.world().destruction_model(reference);
    let world = game.world();
    let record = world.record(base).unwrap();
    let model_indices = record
        .all(T(b"DSTD"))
        .map(|bytes| bytes.get(2).copied())
        .collect::<Vec<_>>();
    let model_count = record.all(T(b"DMDL")).count();
    assert!(
        model.is_some(),
        "vanilla base {base} selected no stage model, indices={model_indices:?}, model_count={model_count}"
    );
    drop(world);
    let save = game.world().save_state();
    let restored = game;
    restored.world_mut().load_state(&save).unwrap();
    assert!(restored.world_mut().destruction_stage(reference) >= 0);
}
