// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for UI-03's `magic_menu` data path
//! (`crates/godot-host/src/magic_menu.rs`, `crates/game/src/world/spell_hand.rs`).
//!
//! `World::actor_ref` is documented as "the actor's state when it has been
//! built, without building it" (`crates/game/src/world/actors.rs`,
//! `actor_ref`), and only the mutable `World::actor` builds it from the
//! `NPC_` base on first use (same file, `actor` doc comment). `magic_menu::
//! entries` reads the known-spell list through `world.actor_ref(holder)`
//! only, never through the mutable `World::actor` that `PlayerNode::
//! magic_content` (`crates/godot-host/src/godot_player/magic.rs`) also never
//! calls before it. `crates/godot-host/src/inventory_menu.rs` avoids this by
//! reading the item list from `reference.inventory`, a store that exists
//! independent of `ActorState`. The spell list has no such independent
//! store, so it inherits the "unbuilt actor" gap that inventory does not.
//!
//! `docs/godot-host.md`'s native menu framework section and the UI-03 done
//! check both require "known spells list", so an actor's own known,
//! player-castable spell missing from that list on the menu's first open is
//! a defect against that contract, not a design choice.
//!
//! Separately, several sibling readers in the same crate
//! (`crates/game/src/world/actor_spells.rs`'s `spell_list`,
//! `crates/game/src/world/weapon_hands.rs`'s `weapon_hand_slots`) reject a
//! deleted record before trusting its fields. `World::spell`
//! (`crates/game/src/world/magic.rs`) and `spell_hand::equip_hand_spell`
//! never check `Record::is_deleted`, so a `SPEL` record whose winning
//! override is flagged deleted but still carries a full `SPIT` block (the
//! shape xEdit's "Undelete and Disable" leaves behind, and what a careless
//! patch can produce) is neither excluded from the menu nor rejected as an
//! equip target.

use soe_esm::{plugin::flags, FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::combat::Hand;
use soe_game::Game;
use soe_godot_host::magic_menu;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x100;
const NPC_BASE: u32 = 0x500;
const CASTABLE_SPELL: u32 = 0x300;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A minimal SPIT block: byte 8 carries the spell type, 0 for the
/// player-castable "Spell" kind (`TES5Edit/Core/wbDefinitionsTES5.pas`,
/// `wbRecord('SPIT'`).
fn spit(spell_type: u32) -> Vec<u8> {
    let mut data = vec![0; 36];
    put(&mut data, 8, spell_type);
    data
}

/// An `NPC_` base that grants `CASTABLE_SPELL` directly through its own
/// `SPLO` subrecord (`uesp-modfileformat/NPC.wikitext`), so the actor's
/// known-spell set is populated the first time anything builds its state,
/// with no manual `actor().spells.insert` needed.
fn game_with_spell_granting_actor() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), NPC_BASE)
            .sub(T(b"ACBS"), vec![0; 24])
            .sub(T(b"SPLO"), CASTABLE_SPELL.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), NPC_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"SPEL"), CASTABLE_SPELL).sub(T(b"SPIT"), spit(0)));
    let mut order = LoadOrder::new();
    order.add("UiMagicAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn entries_omits_a_known_castable_spell_before_the_actor_state_is_built() {
    let game = game_with_spell_granting_actor();

    // Nothing has touched the actor mutably yet: no combat, no inventory,
    // no actor-value read, exactly PlayerNode's own sequence on
    // `open_magic_menu` (`game.player()` then `magic_menu::entries` with no
    // step between). The base grants one player-castable spell, so the
    // menu's own contract ("known spells list") requires it to show here.
    let entries = magic_menu::entries(&game, id(ACTOR));

    assert_eq!(
        entries.len(),
        1,
        "the actor's NPC_ base grants a player-castable spell, so a first \
         call to magic_menu::entries should list it; it instead returns {} \
         entries because entries() reads World::actor_ref, which never \
         builds ActorState, while the spell only exists in the state that \
         World::actor (never called on this path) builds from the base",
        entries.len()
    );
}

#[test]
fn reselecting_a_hand_on_the_still_unbuilt_actor_never_recovers_the_list() {
    let game = game_with_spell_granting_actor();

    // `PlayerNode::select_magic_hand` redraws through the same
    // `magic_content` -> `magic_menu::entries` call as the initial open, so
    // clicking "Left Hand" then "Right Hand" a few times is the closest
    // in-crate proxy for repeated menu interaction without a live Godot
    // process. If the empty list were a one-frame race, a later call would
    // pick up the built state. Nothing here ever builds it, so it should
    // stay wrong across every redraw, not just the first.
    for _ in 0..3 {
        let entries = magic_menu::entries(&game, id(ACTOR));
        assert_eq!(
            entries.len(),
            1,
            "known spell missing from every redraw of an actor whose state \
             was never built, not only the first"
        );
    }
}

#[test]
fn a_deleted_spell_record_is_still_listed_and_equippable() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), NPC_BASE).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), NPC_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    // Deleted, but its SPIT block is still fully present, the shape left by
    // xEdit's "Undelete and Disable References" and by a careless patch
    // that flags a record deleted without stripping its data.
    plugin.push_top(
        Record::new(T(b"SPEL"), CASTABLE_SPELL)
            .sub(T(b"SPIT"), spit(0))
            .with_flags(flags::DELETED),
    );
    let mut order = LoadOrder::new();
    order.add("UiMagicDeleted.esm", plugin).unwrap();
    let mut game = Game::new(order);

    // Build the actor's state up front and grant the spell directly, the
    // same sequence the sibling `ui_03_magic_menu.rs` unit tests use, so
    // this test isolates the deleted-record gap from the cold-actor gap
    // above.
    {
        let mut world = game.world_mut();
        world
            .actor(id(ACTOR))
            .unwrap()
            .spells
            .insert(id(CASTABLE_SPELL));
    }

    let entries = magic_menu::entries(&game, id(ACTOR));
    assert!(
        entries.is_empty(),
        "a SPEL record whose winning override is flagged deleted should not \
         reach the known-spell list, the way a deleted LVSP is rejected by \
         World::spell_list and a deleted DOBJ root is skipped by \
         World::weapon_hand_slots; entries() lists it anyway: {entries:?}"
    );

    let equipped = magic_menu::equip_spell(&mut game, Hand::Right, id(CASTABLE_SPELL));
    assert!(
        !equipped,
        "equip_hand_spell should reject a deleted SPEL the same way it \
         rejects a missing one, not silently equip it to a hand"
    );
}
