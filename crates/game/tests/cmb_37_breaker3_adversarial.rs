// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! CMB-37 flag behavior when one damage call reaches later DEST stages.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::{ObjectId, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn stage(percent: u8, flags: u8) -> Vec<u8> {
    let mut data = vec![percent, 0, 1, flags];
    data.extend_from_slice(&[0; 16]);
    data
}

fn game(stages: &[(u8, u8)]) -> Game {
    let mut base = Record::new(T(b"ACTI"), BASE).sub(
        T(b"DEST"),
        [
            100i32.to_le_bytes().as_slice(),
            &[stages.len() as u8, 0, 0, 0],
        ]
        .concat(),
    );
    for (percent, flags) in stages {
        base = base
            .sub(T(b"DSTD"), stage(*percent, *flags))
            .sub(T(b"DMDL"), b"meshes/stage.nif\0".to_vec())
            .sub(T(b"DSTF"), Vec::new());
    }
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base);
    let mut order = LoadOrder::new();
    order.add("Cmb37Flags.esm", plugin).unwrap();
    Game::new(order)
}

fn damage(game: &mut Game, reference: GlobalFormId, amount: f32) {
    game.world_mut()
        .damage_object(reference, amount, &mut |_, _| ObjectId(1));
}

fn reference(game: &mut Game) -> GlobalFormId {
    let player = game.player();
    game.world_mut().place_at(id(BASE), player, false)
}

fn stage_events(game: &Game) -> Vec<Vec<Value>> {
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
fn cmb_37_later_ignore_external_damage_stage_stops_a_large_hit() {
    // xEdit names DSTD flag bit 3 "Ignore External Dmg".
    let mut game = game(&[(75, 0), (25, 8)]);
    let reference = reference(&mut game);

    damage(&mut game, reference, 100.0);

    assert_eq!(game.world_mut().destruction_stage(reference), 0);
    assert_eq!(
        game.world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        Some(75.0)
    );
    assert_eq!(stage_events(&game), [vec![Value::Int(-1), Value::Int(0)]]);
}

#[test]
fn cmb_37_later_cap_damage_stage_limits_health_at_its_threshold() {
    // xEdit names DSTD flag bit 0 "Cap Damage".
    let mut game = game(&[(75, 0), (50, 1), (25, 0)]);
    let reference = reference(&mut game);

    damage(&mut game, reference, 60.0);

    assert_eq!(game.world_mut().destruction_stage(reference), 1);
    assert_eq!(
        game.world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        Some(50.0)
    );
    assert_eq!(
        stage_events(&game),
        [
            vec![Value::Int(-1), Value::Int(0)],
            vec![Value::Int(0), Value::Int(1)]
        ]
    );
}
