// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::ReferenceChanges;
use soe_game::{Game, Pending};
use soe_papyrus::{ObjectId, Value};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const BASE: u32 = 0x100;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut base = Record::new(T(b"ACTI"), BASE).sub(
        T(b"DEST"),
        [100i32.to_le_bytes().as_slice(), &[1, 0, 0, 0]].concat(),
    );
    let mut dstd = vec![50, 0, 1, 0];
    dstd.extend_from_slice(&[0; 16]);
    base = base
        .sub(T(b"DSTD"), dstd)
        .sub(T(b"DMDL"), b"meshes/stage.nif\0".to_vec());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base);
    let mut order = LoadOrder::new();
    order.add("Cmb37Clear.esm", plugin).unwrap();
    Game::new(order)
}

fn damage(game: &mut Game, reference: GlobalFormId, amount: f32) {
    game.world_mut()
        .damage_object(reference, amount, &mut |_, _| ObjectId(1));
}

fn stage_events(game: &Game) -> Vec<Vec<Value>> {
    game.world()
        .pending
        .iter()
        .filter_map(|event| match event {
            Pending::Event { name, args, .. } if name == "OnDestructionStageChanged" => {
                Some(args.clone())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn cmb_37_clear_resets_damage_without_event_and_keeps_ck_repeat_semantics() {
    let mut game = game();
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    damage(&mut game, reference, 50.0);
    assert_eq!(game.world_mut().destruction_stage(reference), 0);
    assert_eq!(stage_events(&game).len(), 1);

    game.world_mut()
        .clear_destruction(reference, &mut |_, _| ObjectId(1));
    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(
        game.world()
            .reference_ref(reference)
            .unwrap()
            .destruction_health,
        Some(100.0)
    );
    assert_eq!(stage_events(&game).len(), 1);

    damage(&mut game, reference, 100.0);
    assert_eq!(game.world_mut().destruction_stage(reference), -1);
    assert_eq!(stage_events(&game).len(), 1);
}

#[test]
fn cmb_37_damage_invalidates_a_watched_presentation_for_model_replacement() {
    let mut game = game();
    let player = game.player();
    let reference = game.world_mut().place_at(id(BASE), player, false);
    let mut changes = ReferenceChanges::default();
    changes.watch(&game.world(), reference);
    assert_eq!(changes.changed(&game.world()), [reference]);

    damage(&mut game, reference, 50.0);

    assert_eq!(changes.changed(&game.world()), [reference]);
}
