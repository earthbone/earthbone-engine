// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! CMB-37 edge cases from CK destruction events and xEdit DEST records.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, Pending};
use soe_papyrus::{ObjectId, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn stage(percent: u8, flags: u8) -> Vec<u8> {
    let mut data = vec![percent, 0, 0, flags];
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

fn base(count: u8, stages: &[(u8, u8)]) -> Record {
    let dest = [100i32.to_le_bytes().as_slice(), &[count, 0, 0, 0]].concat();
    let mut record = Record::new(T(b"ACTI"), BASE).sub(T(b"DEST"), dest);
    for (percent, flags) in stages {
        record = record.sub(T(b"DSTD"), stage(*percent, *flags));
        record = record.sub(T(b"DSTF"), Vec::new());
    }
    record
}

fn damage(game: &mut Game, reference: GlobalFormId, amount: f32) {
    game.world_mut()
        .damage_object(reference, amount, &mut |_, _| ObjectId(1));
}

fn reference(game: &mut Game) -> GlobalFormId {
    let player = game.player();
    game.world_mut().place_at(id(BASE), player, false)
}

fn destruction_events(game: &Game) -> Vec<Vec<Value>> {
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
fn cmb_37_stage_event_carries_old_and_current_stage() {
    let mut game = game(base(2, &[(75, 0), (25, 0)]));
    let reference = reference(&mut game);

    damage(&mut game, reference, 30.0);

    assert_eq!(
        destruction_events(&game),
        [vec![Value::Int(-1), Value::Int(0)]]
    );
}

#[test]
fn cmb_37_malformed_stage_count_is_ignored_without_state_change() {
    let mut game = game(base(2, &[(75, 0)]));
    let reference = reference(&mut game);

    damage(&mut game, reference, 50.0);

    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert!(game.world().pending.is_empty());
}

#[test]
fn cmb_37_cap_damage_stops_at_stage_threshold_and_later_damage_resumes() {
    let mut game = game(base(2, &[(75, 1), (25, 0)]));
    let reference = reference(&mut game);

    damage(&mut game, reference, 90.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 0);
    assert_eq!(
        game.world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        Some(75.0)
    );

    damage(&mut game, reference, 50.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 1);
}

#[test]
fn cmb_37_invalid_damage_does_not_mutate_health_or_queue_events() {
    let mut game = game(base(1, &[(50, 0)]));
    let reference = reference(&mut game);

    for amount in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        damage(&mut game, reference, amount);
    }

    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(
        game.world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        None
    );
    assert!(destruction_events(&game).is_empty());
}

#[test]
fn cmb_37_ignore_external_damage_keeps_reference_undamaged() {
    let mut game = game(base(1, &[(50, 8)]));
    let reference = reference(&mut game);

    damage(&mut game, reference, 100.0);

    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert!(destruction_events(&game).is_empty());
}

#[test]
fn cmb_37_saved_health_continues_toward_the_next_stage() {
    let mut original = game(base(2, &[(75, 0), (25, 0)]));
    let reference = reference(&mut original);
    damage(&mut original, reference, 30.0);
    let saved = original.world().save_state();

    let mut restored = game(base(2, &[(75, 0), (25, 0)]));
    restored.world_mut().load_state(&saved).unwrap();
    assert_eq!(restored.world_mut().destruction_stage(reference), 0);
    assert_eq!(
        restored
            .world_mut()
            .reference(reference)
            .unwrap()
            .destruction_health,
        Some(70.0)
    );

    damage(&mut restored, reference, 50.0);
    assert_eq!(restored.world_mut().destruction_stage(reference), 1);
}
