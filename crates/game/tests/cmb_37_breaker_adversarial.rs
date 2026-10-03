// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-37 adversarial checks from CK destruction natives and xEdit DEST layout.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn stage(percent: u8) -> Vec<u8> {
    let mut data = vec![percent, 0, 1, 0];
    data.extend_from_slice(&[0; 16]);
    data
}

fn game(base: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base);
    let mut order = LoadOrder::new();
    order.add("Cmb37.esm", plugin).unwrap();
    Game::new(order)
}

fn destructive_base(stages: &[u8]) -> Record {
    let mut record = Record::new(T(b"ACTI"), BASE).sub(
        T(b"DEST"),
        [
            100i32.to_le_bytes().as_slice(),
            &[stages.len() as u8, 0, 0, 0],
        ]
        .concat(),
    );
    for (index, percent) in stages.iter().copied().enumerate() {
        record = record.sub(T(b"DSTD"), stage(percent));
        if index == 1 {
            record = record.sub(T(b"DMDL"), b"meshes/later-stage.nif\0".to_vec());
        }
    }
    record
}

fn damage(game: &mut Game, reference: GlobalFormId, amount: f32) {
    game.world_mut()
        .damage_object(reference, amount, &mut |_, _| ObjectId(1));
}

#[test]
fn cmb_37_short_dest_header_is_rejected_without_panicking() {
    let mut game =
        game(Record::new(T(b"ACTI"), BASE).sub(T(b"DEST"), 100i32.to_le_bytes().to_vec()));
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    let result = catch_unwind(AssertUnwindSafe(|| damage(&mut game, reference, 1.0)));
    assert!(result.is_ok(), "malformed DEST must be ignored safely");
    assert_eq!(game.world_mut().destruction_stage(reference), -1);
}

#[test]
fn cmb_37_missing_earlier_model_does_not_shift_later_stage_model() {
    let mut game = game(destructive_base(&[75, 25]));
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    damage(&mut game, reference, 30.0);
    assert_eq!(game.world().destruction_model(reference), None);
    damage(&mut game, reference, 50.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 1);
    assert_eq!(
        game.world().destruction_model(reference).as_deref(),
        Some("meshes/later-stage.nif")
    );
}

#[test]
fn cmb_37_damage_crossing_multiple_stages_emits_each_stage_in_order() {
    let mut base = destructive_base(&[75, 50, 25]);
    base = base.sub(T(b"DMDL"), b"meshes/first-stage.nif\0".to_vec());
    let mut game = game(base);
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    damage(&mut game, reference, 100.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 2);
    let transitions: Vec<_> = game
        .world()
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnDestructionStageChanged" => {
                Some(args.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        transitions,
        [
            vec![soe_papyrus::Value::Int(-1), soe_papyrus::Value::Int(0)],
            vec![soe_papyrus::Value::Int(0), soe_papyrus::Value::Int(1)],
            vec![soe_papyrus::Value::Int(1), soe_papyrus::Value::Int(2)],
        ]
    );
}

#[test]
fn cmb_37_health_and_stage_survive_save_load_then_clear_resets_them() {
    let mut original = game(destructive_base(&[75, 25]));
    let player = original.player();
    let reference = original.world_mut().place_at(id(BASE), player, false);
    damage(&mut original, reference, 30.0);
    let saved = original.world().save_state();

    let mut restored = game(destructive_base(&[75, 25]));
    restored.world_mut().load_state(&saved).unwrap();
    {
        let mut world = restored.world_mut();
        let state = world.reference(reference).unwrap();
        assert_eq!(state.destruction_stage, 0);
        assert_eq!(state.destruction_health, Some(70.0));
    }
    damage(&mut restored, reference, 50.0);
    assert_eq!(restored.world_mut().destruction_stage(reference), 1);
    restored
        .world_mut()
        .clear_destruction(reference, &mut |_, _| ObjectId(1));
    assert_eq!(restored.world_mut().destruction_stage(reference), -1);
    assert_eq!(
        restored
            .world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        Some(100.0)
    );
}

#[test]
fn cmb_37_installed_vanilla_destructible_advances_and_survives_save_load() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed CMB-37 case");
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
    let shared_order = std::sync::Arc::new(order);
    let mut runtime = Game::from_shared_order(std::sync::Arc::clone(&shared_order));
    let base = runtime
        .world()
        .order()
        .winners_of(T(b"ACTI"))
        .find_map(|(id, record)| {
            (!record.is_deleted()
                && record.find(T(b"DEST")).is_some_and(|dest| dest.len() >= 8)
                && record.all(T(b"DSTD")).next().is_some())
            .then_some(id)
        })
        .expect("installed masters contain a vanilla destructible ACTI");
    let player = runtime.player();
    let reference = runtime.world_mut().place_at(base, player, false);
    for _ in 0..16 {
        damage(&mut runtime, reference, 100_000.0);
    }
    assert!(runtime.world_mut().destruction_stage(reference) >= 0);
    let saved = runtime.world().save_state();
    let restored = Game::from_shared_order(shared_order);
    restored.world_mut().load_state(&saved).unwrap();
    assert!(restored.world_mut().destruction_stage(reference) >= 0);
}
