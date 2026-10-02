// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-16 reference translation behavior using a synthetic placed reference.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::world::{Pending, TranslationPath};
use soe_papyrus::ObjectId;

const REFERENCE: u32 = 0x900;

fn id() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: REFERENCE,
    }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    let transform = [0.0f32; 6];
    plugin.push_top(
        Record::new(Tag(*b"REFR"), REFERENCE)
            .sub(Tag(*b"NAME"), 0x800u32.to_le_bytes())
            .sub(
                Tag(*b"DATA"),
                transform
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ),
    );
    let mut order = LoadOrder::new();
    order.add("Translation.esm", plugin).unwrap();
    Game::new(order)
}

fn tick(game: &mut Game, seconds: f64) {
    game.world_mut()
        .tick_translations(seconds, &mut |_, _| ObjectId(1));
}

fn events(game: &Game) -> Vec<String> {
    game.world()
        .pending
        .iter()
        .filter_map(|pending| match pending {
            Pending::Event { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn hst_16_linear_translation_uses_speed_and_waits_for_rotation() {
    let mut game = fixture();
    game.world_mut().start_translation(
        id(),
        [10.0, 0.0, 0.0],
        [0.0, 0.0, std::f32::consts::PI],
        10.0,
        90.0,
        TranslationPath::Linear,
        &mut |_, _| ObjectId(1),
    );

    tick(&mut game, 0.5);
    let world = game.world();
    let reference = world.reference_ref(id()).unwrap();
    assert_eq!(reference.position, [5.0, 0.0, 0.0]);
    assert!((reference.rotation[2] - std::f32::consts::FRAC_PI_4).abs() < 0.0001);
    assert!(events(&game).is_empty());
    drop(world);

    tick(&mut game, 0.5);
    assert_eq!(
        game.world().reference_ref(id()).unwrap().position,
        [10.0, 0.0, 0.0]
    );
    assert_eq!(events(&game), ["OnTranslationAlmostComplete"]);
    tick(&mut game, 1.0);
    assert_eq!(
        game.world().reference_ref(id()).unwrap().position,
        [10.0, 0.0, 0.0]
    );
    assert_eq!(
        events(&game),
        ["OnTranslationAlmostComplete", "OnTranslationComplete"]
    );
}

#[test]
fn hst_16_spline_translation_reaches_both_targets_and_completes() {
    let mut game = fixture();
    game.world_mut().start_translation(
        id(),
        [10.0, 0.0, 0.0],
        [0.0, 0.0, 1.0],
        10.0,
        0.0,
        TranslationPath::Spline {
            tangent_magnitude: 2.0,
        },
        &mut |_, _| ObjectId(1),
    );
    tick(&mut game, 0.5);
    assert_ne!(game.world().reference_ref(id()).unwrap().position[1], 0.0);
    assert!(events(&game).is_empty());
    tick(&mut game, 10.0);

    let reference = game.world().reference_ref(id()).unwrap().clone();
    assert_eq!(reference.position, [10.0, 0.0, 0.0]);
    assert_eq!(reference.rotation, [0.0, 0.0, 1.0]);
    assert_eq!(
        events(&game),
        ["OnTranslationAlmostComplete", "OnTranslationComplete"]
    );
}

#[test]
fn hst_16_stopping_translation_keeps_position_and_fires_failure() {
    let mut game = fixture();
    game.world_mut().start_translation(
        id(),
        [10.0, 0.0, 0.0],
        [0.0; 3],
        10.0,
        0.0,
        TranslationPath::Linear,
        &mut |_, _| ObjectId(1),
    );
    tick(&mut game, 0.25);
    let stopped_at = game.world().reference_ref(id()).unwrap().position;
    assert!(
        game.world_mut()
            .stop_translation(id(), &mut |_, _| ObjectId(1))
    );
    tick(&mut game, 1.0);

    assert_eq!(
        game.world().reference_ref(id()).unwrap().position,
        stopped_at
    );
    assert_eq!(events(&game), ["OnTranslationFailed"]);
}

#[test]
fn hst_16_disabled_translation_fires_failure_without_moving() {
    let mut game = fixture();
    game.world_mut().start_translation(
        id(),
        [10.0, 0.0, 0.0],
        [0.0; 3],
        10.0,
        0.0,
        TranslationPath::Linear,
        &mut |_, _| ObjectId(1),
    );
    game.world_mut().reference(id()).unwrap().enabled = false;
    tick(&mut game, 0.5);

    assert_eq!(game.world().reference_ref(id()).unwrap().position, [0.0; 3]);
    assert_eq!(events(&game), ["OnTranslationFailed"]);
}

#[test]
fn hst_16_blocked_translation_stops_at_collision_and_fires_failure() {
    let mut game = fixture();
    game.world_mut().start_translation(
        id(),
        [10.0, 0.0, 0.0],
        [0.0; 3],
        10.0,
        0.0,
        TranslationPath::Linear,
        &mut |_, _| ObjectId(1),
    );
    tick(&mut game, 0.25);
    let blocked_at = game.world().reference_ref(id()).unwrap().position;
    assert!(
        game.world_mut()
            .block_translation(id(), &mut |_, _| ObjectId(1))
    );

    tick(&mut game, 1.0);
    assert_eq!(
        game.world().reference_ref(id()).unwrap().position,
        blocked_at
    );
    assert_eq!(events(&game), ["OnTranslationFailed"]);
}

#[test]
fn hst_16_in_flight_translation_survives_reference_save_restore() {
    let mut original = fixture();
    original.world_mut().start_translation(
        id(),
        [10.0, 2.0, 0.0],
        [0.0, 0.0, 1.0],
        4.0,
        20.0,
        TranslationPath::Spline {
            tangent_magnitude: 3.0,
        },
        &mut |_, _| ObjectId(1),
    );
    tick(&mut original, 0.5);
    let expected = original.world().reference_ref(id()).unwrap().translation;
    let bytes = original.save_state().unwrap();

    let mut restored = fixture();
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.world().reference_ref(id()).unwrap().translation,
        expected
    );
}

#[test]
fn hst_16_replacing_translation_fires_failure_for_old_request() {
    let game = fixture();
    game.world_mut().start_translation(
        id(),
        [10.0, 0.0, 0.0],
        [0.0; 3],
        10.0,
        0.0,
        TranslationPath::Linear,
        &mut |_, _| ObjectId(1),
    );
    game.world_mut().start_translation(
        id(),
        [0.0, 10.0, 0.0],
        [0.0; 3],
        10.0,
        0.0,
        TranslationPath::Linear,
        &mut |_, _| ObjectId(1),
    );

    assert_eq!(events(&game), ["OnTranslationFailed"]);
}
