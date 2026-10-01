// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! DLG-17 covers recruitment, dismissal and follower package ownership.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{ai, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x800;
const BASE: u32 = 0x801;
const FACTION: u32 = 0x802;
const PRIOR_PACKAGE: u32 = 0x803;
const FOLLOW_PACKAGE: u32 = 0x804;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(Record::new(T(b"FACT"), FACTION));
    plugin.push_top(Record::new(T(b"PACK"), PRIOR_PACKAGE));
    plugin.push_top(Record::new(T(b"PACK"), FOLLOW_PACKAGE));
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .sub(T(b"ACBS"), vec![0; 24])
            .sub(T(b"PKID"), PRIOR_PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Follower.esm", plugin).unwrap();
    order
}

fn game() -> Game {
    let game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn dlg_17_recruitment_sets_teammate_favor_service_and_package_owner() {
    let game = game();
    game.world_mut()
        .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), true)
        .unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.teammate);
    assert!(actor.can_do_favors);
    assert_eq!(actor.factions.get(&id(FACTION)), Some(&0));
    let candidates = ai::candidates(&mut world, id(ACTOR));
    assert_eq!(candidates[0].package, id(FOLLOW_PACKAGE));
    assert_eq!(candidates[1].package, id(PRIOR_PACKAGE));
}

#[test]
fn dlg_17_dismissal_restores_prior_service_state_and_package_owner() {
    let game = game();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.teammate = true;
        actor.can_do_favors = false;
        actor.factions.insert(id(FACTION), 7);
    }
    game.world_mut()
        .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), true)
        .unwrap();
    game.world_mut().dismiss_follower(id(ACTOR)).unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.teammate);
    assert!(!actor.can_do_favors);
    assert_eq!(actor.factions.get(&id(FACTION)), Some(&7));
    assert_eq!(world.follower_package(id(ACTOR)), None);
    assert_eq!(
        ai::candidates(&mut world, id(ACTOR))[0].package,
        id(PRIOR_PACKAGE)
    );
}

#[test]
fn dlg_17_dismissal_removes_service_faction_added_by_recruitment() {
    let game = game();
    game.world_mut()
        .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), false)
        .unwrap();
    game.world_mut().dismiss_follower(id(ACTOR)).unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(!actor.teammate);
    assert!(!actor.can_do_favors);
    assert!(!actor.factions.contains_key(&id(FACTION)));
}

#[test]
fn dlg_17_recruited_service_state_round_trips_world_save() {
    let game = Game::new(order());
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.world_mut()
        .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), true)
        .unwrap();
    let saved = game.world().save_state();

    let loaded = Game::new(order());
    loaded.world_mut().load_state(&saved).unwrap();
    assert_eq!(
        loaded.world().follower_package(id(ACTOR)),
        Some(id(FOLLOW_PACKAGE))
    );
    let mut world = loaded.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.teammate);
    assert!(actor.can_do_favors);
    assert_eq!(actor.factions.get(&id(FACTION)), Some(&0));
}

#[test]
fn dlg_17_recruiting_twice_preserves_the_original_restore_values() {
    let game = game();
    assert!(matches!(
        game.world_mut()
            .recruit_follower(id(ACTOR), id(0x999), id(FOLLOW_PACKAGE), true),
        Err(soe_game::services::FollowerError::Faction(_))
    ));
    assert!(matches!(
        game.world_mut()
            .recruit_follower(id(ACTOR), id(FACTION), id(0x999), true),
        Err(soe_game::services::FollowerError::Package(_))
    ));
    game.world_mut()
        .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), true)
        .unwrap();
    let second =
        game.world_mut()
            .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), false);
    assert!(matches!(
        second,
        Err(soe_game::services::FollowerError::AlreadyRecruited(_))
    ));
    game.world_mut().dismiss_follower(id(ACTOR)).unwrap();

    let mut world = game.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(!actor.teammate);
    assert!(!actor.can_do_favors);
    assert!(!actor.factions.contains_key(&id(FACTION)));
    assert_eq!(
        ai::candidates(&mut world, id(ACTOR))[0].package,
        id(PRIOR_PACKAGE)
    );
}

#[test]
fn dlg_17_dismissal_after_save_load_restores_pre_recruitment_state() {
    let game = game();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.teammate = true;
        actor.can_do_favors = false;
        actor.factions.insert(id(FACTION), -1);
    }
    game.world_mut()
        .recruit_follower(id(ACTOR), id(FACTION), id(FOLLOW_PACKAGE), true)
        .unwrap();
    let saved = game.world().save_state();
    let loaded = Game::new(order());
    loaded.world_mut().load_state(&saved).unwrap();
    loaded.world_mut().dismiss_follower(id(ACTOR)).unwrap();

    let mut world = loaded.world_mut();
    let actor = world.actor(id(ACTOR)).unwrap();
    assert!(actor.teammate);
    assert!(!actor.can_do_favors);
    assert_eq!(actor.factions.get(&id(FACTION)), Some(&-1));
    assert_eq!(world.follower_package(id(ACTOR)), None);
}
