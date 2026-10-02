// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{
    ActivateRefChildren, AliasInstances, Calendar as CalendarApi, Character as CharacterApi,
    DefaultObjects as DefaultObjectsApi, UnsupportedHook,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[test]
fn shm_07_calendar_reads_the_live_world_clock() {
    let game = Game::new(LoadOrder::new());
    let mut world = game.world_mut();
    world.advance_clock(4320.0);
    let view = world.shim_calendar();
    assert_eq!((view.year(), view.month(), view.day()), (201, 7, 18));
    assert_eq!(view.hour(), 0.0);
    assert_eq!(view.days_passed(), 1.0);
    assert_eq!(view.timescale(), 20.0);
}

#[test]
fn shm_07_character_and_other_world_contracts_use_current_engine_state() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"DOBJ"), 0x31).sub(
        T(b"DNAM"),
        [b"DFTS".as_slice(), &0x800_u32.to_le_bytes()].concat(),
    ));
    plugin.push_top(Record::new(T(b"MISC"), 0x800).zsub(T(b"EDID"), "Footsteps"));
    plugin.push_top(Record::new(T(b"PACK"), 0x801).zsub(T(b"EDID"), "AliasPackage"));
    plugin.push_top(quest::with_alias_packages(
        quest::build_record(0x802, "AliasQuest", 0),
        4,
        "Target",
        None,
        &[0x801],
    ));
    let mut order = LoadOrder::new();
    order.add("Test.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let actor = game.player();
    {
        let mut world = game.world_mut();
        world.actor(actor).expect("player actor state exists");
        let view = world.shim_character(actor).expect("player character view");
        assert_eq!(view.form_id(), FormId(0x14));
        assert_eq!(view.actor_value("health"), Some(100.0));
        assert!(!view.is_dead());
        assert_eq!(view.race(), None);

        let default = world.shim_default_objects().get(*b"DFTS").unwrap().unwrap();
        assert_eq!(
            world
                .editor_id(world.from_runtime_id(default.0).unwrap())
                .as_deref(),
            Some("Footsteps")
        );
        let quest = world.form("AliasQuest").unwrap();
        let target = GlobalFormId {
            file: 0,
            object: 0x14,
        };
        assert!(world.force_alias(quest, 4, Some(target)));
        let instances = world.instances(FormId(0x14));
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].quest, FormId(world.runtime_id(quest)));
        assert_eq!(instances[0].alias_id, 4);
        assert_eq!(instances[0].reference, Some(FormId(0x14)));
        assert_eq!(instances[0].packages, vec![FormId(0x801)]);
        assert_eq!(
            world.children(FormId(0x14)),
            Err(UnsupportedHook::ActivateReferenceChildren)
        );
        assert_eq!(
            world.timer(FormId(0x14)),
            Err(UnsupportedHook::ActivateReferenceChildren)
        );
        assert_eq!(
            world.activate_children(FormId(0x14)),
            Err(UnsupportedHook::ActivateReferenceChildren)
        );
    }
}
