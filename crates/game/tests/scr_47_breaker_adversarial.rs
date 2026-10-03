// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::{SpellArtAction, SpellArtRequest};
use soe_game::Game;

fn id() -> GlobalFormId {
    GlobalFormId { file: 0, object: 0x800 }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"SPEL"), 0x800).sub(Tag(*b"SPIT"), vec![0; 36]));
    let mut order = LoadOrder::new();
    order.add("Spell.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn scr_47_unload_without_a_preload_does_not_queue_a_request() {
    let mut game = game();
    assert!(game.world_mut().unload_spell_art(id()).is_ok());
    assert!(game.world_mut().preload_spell_art(id()).is_ok());
    assert!(game.world_mut().unload_spell_art(id()).is_ok());
    assert!(game.world_mut().unload_spell_art(id()).is_ok());
    assert_eq!(
        game.world_mut().take_spell_art_requests(),
        vec![
            SpellArtRequest { spell: id(), action: SpellArtAction::Preload },
            SpellArtRequest { spell: id(), action: SpellArtAction::Unload },
        ]
    );
}
