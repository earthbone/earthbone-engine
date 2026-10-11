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
    for object in [0x800, 0x801] {
        plugin.push_top(Record::new(Tag(*b"SPEL"), object).sub(Tag(*b"SPIT"), vec![0; 36]));
    }
    plugin.push_top(Record::new(Tag(*b"MGEF"), 0x802));
    let mut order = LoadOrder::new();
    order.add("SpellArt.esp", plugin).expect("load test plugin");
    Game::new(order)
}

#[test]
fn scr_47_spell_art_requests_keep_interleaved_spell_identity_and_order() {
    let game = game();
    let first = id(0x800);
    let second = id(0x801);

    game.world_mut().preload_spell_art(first).unwrap();
    game.world_mut().preload_spell_art(second).unwrap();
    game.world_mut().unload_spell_art(first).unwrap();
    game.world_mut().unload_spell_art(second).unwrap();

    assert_eq!(
        game.world_mut().take_spell_art_requests(),
        vec![
            SpellArtRequest { spell: first, action: SpellArtAction::Preload },
            SpellArtRequest { spell: second, action: SpellArtAction::Preload },
            SpellArtRequest { spell: first, action: SpellArtAction::Unload },
            SpellArtRequest { spell: second, action: SpellArtAction::Unload },
        ]
    );
    assert!(game.world_mut().take_spell_art_requests().is_empty());
    assert_eq!(game.world().spell_casts.len(), 0);
    assert_eq!(game.world().active_effects().count(), 0);
}

#[test]
fn scr_47_invalid_unload_does_not_consume_a_valid_spell_preload() {
    let game = game();
    let spell = id(0x800);

    game.world_mut().preload_spell_art(spell).unwrap();
    assert!(game.world_mut().unload_spell_art(id(0x802)).is_err());
    game.world_mut().unload_spell_art(spell).unwrap();

    assert_eq!(
        game.world_mut().take_spell_art_requests(),
        vec![
            SpellArtRequest { spell, action: SpellArtAction::Preload },
            SpellArtRequest { spell, action: SpellArtAction::Unload },
        ]
    );
}
