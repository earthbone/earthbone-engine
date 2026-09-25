// SPDX-License-Identifier: GPL-3.0-or-later

//! Form-backed known-spell rows for the native host menu, mirroring
//! `inventory_menu.rs`. SkyUI's magic menu lists known, player-castable
//! spells and lets the player send one to either hand
//! (`skyui/src/ItemMenus/magicmenu.fla`, `skyui/src/Common/`). Only SPIT
//! spell type 0 ("Spell" in xEdit's `wbDefinitionsTES5.pas` SPEL enum) is
//! player-castable that way. Powers, Lesser Powers and Abilities read from
//! the same known-spell set but never occupy a hand, so they stay off this
//! list, matching the CK wiki EquipSpell/UnequipSpell hand argument
//! (0 = left hand, 1 = right hand, 2 = other/voice).

use soe_esm::GlobalFormId;
use soe_game::combat::Hand;
use soe_game::Game;

const CASTABLE_SPELL_TYPE: u32 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub spell: GlobalFormId,
    pub name: String,
    pub cost: u32,
    pub left_equipped: bool,
    pub right_equipped: bool,
}

/// List a holder's known, player-castable spells in form-id order.
pub fn entries(game: &Game, holder: GlobalFormId) -> Vec<Entry> {
    // `World::actor_ref` reads state that only the mutable `World::actor`
    // builds from the `NPC_` base on first use (`crates/game/src/world/
    // actors.rs`). `Game::world_mut` borrows through a `RefCell`, so a
    // shared `&Game` can still force that build before the read below,
    // the way `PlayerNode::open_magic_menu` expects a freshly opened menu
    // to show an actor's known spells on its very first draw.
    game.world_mut().actor(holder);
    let world = game.world();
    let Some(actor) = world.actor_ref(holder) else {
        return Vec::new();
    };
    let left = actor.left_spell;
    let right = actor.right_spell;
    actor
        .spells
        .iter()
        .filter_map(|spell| {
            let data = world.spell(*spell).ok()?;
            (data.data.spell_type == CASTABLE_SPELL_TYPE).then(|| Entry {
                spell: *spell,
                name: world.name_of(*spell).unwrap_or_else(|| format!("{spell}")),
                cost: data.data.base_cost,
                left_equipped: left == Some(*spell),
                right_equipped: right == Some(*spell),
            })
        })
        .collect()
}

/// Send a known spell to a hand through the shared game world.
pub fn equip_spell(game: &mut Game, hand: Hand, spell: GlobalFormId) -> bool {
    let player = game.player();
    game.world_mut()
        .equip_hand_spell(player, hand, spell)
        .is_ok()
}
