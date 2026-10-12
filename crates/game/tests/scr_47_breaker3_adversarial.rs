// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{SpellArtAction, SpellArtRequest};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0x800).sub(Tag(*b"SPIT"), vec![0; 36]));
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0x801).sub(Tag(*b"SPIT"), vec![0; 36]));
    let mut order = LoadOrder::new();
    order.add("Scr47Edge.esp", plugin).expect("load fixture");
    Game::new(order)
}

#[test]
fn scr_47_art_request_drain_is_ordered_and_does_not_repeat_old_work() {
    let mut game = game();
    let first = id(0x800);
    let second = id(0x801);

    game.world_mut().preload_spell_art(first).unwrap();
    game.world_mut().preload_spell_art(second).unwrap();
    game.world_mut().unload_spell_art(first).unwrap();
    game.world_mut().unload_spell_art(second).unwrap();
    let requests = game.world_mut().take_spell_art_requests();

    assert_eq!(
        requests,
        vec![
            SpellArtRequest {
                spell: first,
                action: SpellArtAction::Preload
            },
            SpellArtRequest {
                spell: second,
                action: SpellArtAction::Preload
            },
            SpellArtRequest {
                spell: first,
                action: SpellArtAction::Unload
            },
            SpellArtRequest {
                spell: second,
                action: SpellArtAction::Unload
            },
        ]
    );
    assert!(game.world_mut().take_spell_art_requests().is_empty());
}

#[test]
fn scr_47_invalid_art_request_does_not_consume_another_spells_request() {
    let mut game = game();
    let spell = id(0x800);

    game.world_mut().preload_spell_art(spell).unwrap();
    assert!(game.world_mut().unload_spell_art(id(0x999)).is_err());
    game.world_mut().unload_spell_art(spell).unwrap();

    assert_eq!(
        game.world_mut().take_spell_art_requests(),
        vec![
            SpellArtRequest {
                spell,
                action: SpellArtAction::Preload
            },
            SpellArtRequest {
                spell,
                action: SpellArtAction::Unload
            },
        ]
    );
}

#[test]
fn scr_47_preload_unload_preserve_spell_identity_without_casting() {
    let mut game = game();
    let spell = id(0x801);
    game.world_mut().preload_spell_art(spell).unwrap();
    game.world_mut().unload_spell_art(spell).unwrap();

    assert!(game.world().spell_casts.is_empty());
    assert_eq!(game.world().active_effects().count(), 0);
    assert_eq!(
        game.world_mut().take_spell_art_requests(),
        vec![
            SpellArtRequest {
                spell,
                action: SpellArtAction::Preload
            },
            SpellArtRequest {
                spell,
                action: SpellArtAction::Unload
            },
        ]
    );
}
