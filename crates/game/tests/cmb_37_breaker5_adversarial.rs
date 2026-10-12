// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! CMB-37 checks against the CK destruction natives and xEdit DEST layout.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::{ObjectId, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn stage(percent: u8, model_stage: u8) -> Vec<u8> {
    let mut data = vec![percent, 0, model_stage, 0];
    data.extend_from_slice(&[0; 16]);
    data
}

fn fixture(base: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base);
    let mut order = LoadOrder::new();
    order.add("Cmb37Breaker5.esm", plugin).unwrap();
    Game::new(order)
}

fn destructible(stages: &[(u8, u8, Option<&[u8]>)]) -> Record {
    let dest = [
        100i32.to_le_bytes().as_slice(),
        &[stages.len() as u8, 0, 0, 0],
    ]
    .concat();
    let mut record = Record::new(T(b"ACTI"), BASE).sub(T(b"DEST"), dest);
    for (percent, model_stage, model) in stages {
        record = record.sub(T(b"DSTD"), stage(*percent, *model_stage));
        if let Some(model) = model {
            let mut path = model.to_vec();
            path.push(0);
            record = record.sub(T(b"DMDL"), path);
        }
        record = record.sub(T(b"DSTF"), Vec::new());
    }
    record
}

fn place(game: &mut Game) -> GlobalFormId {
    let player = game.player();
    game.world_mut().place_at(id(BASE), player, false)
}

fn damage(game: &mut Game, reference: GlobalFormId, amount: f32) {
    game.world_mut()
        .damage_object(reference, amount, &mut |_, _| ObjectId(1));
}

fn events(game: &Game) -> Vec<Vec<Value>> {
    game.world()
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, args, .. } if name == "OnDestructionStageChanged" => {
                Some(args.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn cmb_37_breaker5_clear_uses_undamaged_stage_and_suppresses_future_events() {
    let mut game = fixture(destructible(&[(50, 1, Some(b"meshes/damaged.nif"))]));
    let reference = place(&mut game);
    damage(&mut game, reference, 50.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 0);
    assert_eq!(events(&game).len(), 1);

    game.world_mut()
        .clear_destruction(reference, &mut |_, _| ObjectId(1));

    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(game.world().destruction_model(reference), None);
    assert_eq!(events(&game).len(), 1);
    damage(&mut game, reference, 100.0);
    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(events(&game).len(), 1);
}

#[test]
fn cmb_37_breaker5_missing_dest_ignores_damage_without_events() {
    let mut game = fixture(Record::new(T(b"ACTI"), BASE));
    let reference = place(&mut game);

    damage(&mut game, reference, 100.0);

    {
        let mut world = game.world_mut();
        let state = world.reference(reference).unwrap();
        assert_eq!(state.destruction_health, None);
        assert_eq!(state.destruction_stage, -1);
    }
    assert!(events(&game).is_empty());
}

#[test]
fn cmb_37_breaker5_zero_model_index_keeps_last_authored_model() {
    let mut game = fixture(destructible(&[
        (75, 1, Some(b"meshes/stage_one.nif")),
        (25, 0, Some(b"meshes/non_damage_model.nif")),
    ]));
    let reference = place(&mut game);
    damage(&mut game, reference, 80.0);

    assert_eq!(game.world_mut().destruction_stage(reference), 1);
    assert_eq!(
        game.world().destruction_model(reference).as_deref(),
        Some("meshes/stage_one.nif")
    );
    assert_eq!(
        events(&game),
        [
            vec![Value::Int(-1), Value::Int(0)],
            vec![Value::Int(0), Value::Int(1)]
        ]
    );
}

#[test]
fn cmb_37_breaker5_short_stage_record_is_ignored_atomically() {
    let dest = [100i32.to_le_bytes().as_slice(), &[1, 0, 0, 0]].concat();
    let base = Record::new(T(b"ACTI"), BASE)
        .sub(T(b"DEST"), dest)
        .sub(T(b"DSTD"), vec![50, 0, 1, 0])
        .sub(T(b"DMDL"), b"meshes/stage.nif\0".to_vec())
        .sub(T(b"DSTF"), Vec::new());
    let mut game = fixture(base);
    let reference = place(&mut game);

    damage(&mut game, reference, 100.0);

    {
        let mut world = game.world_mut();
        let state = world.reference(reference).unwrap();
        assert_eq!(state.destruction_health, None);
        assert_eq!(state.destruction_stage, -1);
    }
    assert!(events(&game).is_empty());
}
