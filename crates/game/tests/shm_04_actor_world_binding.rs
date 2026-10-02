// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::refr::{
    Actor as ShimActor, ActorApiError, ActorEquipManager, ActorValue, HookError, ObjectRef,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{World, npc};

const BASE: u32 = 0x800;
const ACTOR: u32 = 0x900;
const ITEM: u32 = 0xA00;
const PERK: u32 = 0xA01;
const SPELL: u32 = 0xA02;
const RACE: u32 = 0xA03;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        npc::build_record(
            BASE,
            "ShimActor",
            "ShimActor",
            npc::Config::default(),
            npc::Stats::default(),
        )
        .sub(Tag(*b"RNAM"), RACE.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), ACTOR).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"RACE"), RACE).sub(Tag(*b"DATA"), vec![0; 124]));
    for object in [ITEM, PERK, SPELL] {
        plugin.push_top(Record::new(Tag(*b"STAT"), object));
    }
    let mut order = LoadOrder::new();
    order.add("Shim.esp", plugin).unwrap();
    World::new(order)
}

#[test]
fn shm_04_actor_traits_read_and_mutate_the_shared_world() {
    let mut world = fixture();
    let runtime_actor = world.order().runtime_id(id(ACTOR));
    let runtime_item = world.order().runtime_id(id(ITEM));
    let runtime_perk = world.order().runtime_id(id(PERK));
    let runtime_spell = world.order().runtime_id(id(SPELL));
    let runtime_race = world.order().runtime_id(id(RACE));
    {
        let state = world.actor(id(ACTOR)).unwrap();
        state.sneaking = true;
        state.weapon_drawn = true;
        state.blocking = true;
    }

    let mut actor = soe_game::commonlib::WorldActor::resolve(&mut world, FormId(runtime_actor))
        .expect("placed actor resolves through its runtime FormID");
    assert_eq!(actor.form_id(), FormId(runtime_actor));
    assert_eq!(actor.actor_handle().0, runtime_actor);
    assert_eq!(actor.actor_ptr().0, Some(actor.actor_handle()));
    assert!(actor.state().sneaking);
    assert!(actor.state().weapon_drawn);
    assert!(actor.state().blocking);
    assert_eq!(actor.race(), FormId(runtime_race));
    assert!(!actor.is_dead());
    assert!(!actor.is_player());
    actor.set_actor_value(ActorValue::HEALTH, 67.0).unwrap();
    assert_eq!(actor.actor_value(ActorValue::HEALTH).unwrap(), 67.0);
    assert!(matches!(
        actor.actor_value(ActorValue(99)),
        Err(ActorApiError::UnsupportedOperation)
    ));
    assert!(actor.add_item(FormId(runtime_item), 2));
    assert!(actor.add_perk(FormId(runtime_perk)));
    assert!(actor.add_spell(FormId(runtime_spell)));
    assert!(!actor.add_perk(FormId(runtime_perk)));

    assert_eq!(world.item_count(id(ACTOR), id(ITEM)), 2);
    let state = world.actor_ref(id(ACTOR)).unwrap();
    assert!(state.perks.contains(&id(PERK)));
    assert!(state.spells.contains(&id(SPELL)));
}

#[test]
fn shm_04_actor_equip_manager_changes_shared_equipment_state() {
    let mut world = fixture();
    let runtime_actor = world.order().runtime_id(id(ACTOR));
    let runtime_item = world.order().runtime_id(id(ITEM));
    {
        let mut manager = soe_game::commonlib::WorldActorEquipManager::new(&mut world);
        manager
            .equip_object(
                soe_commonlib_shim::refr::ActorHandle(runtime_actor),
                FormId(runtime_item),
            )
            .unwrap();
    }
    assert!(
        world
            .actor_ref(id(ACTOR))
            .unwrap()
            .equipped
            .contains(&id(ITEM))
    );
    {
        let mut manager = soe_game::commonlib::WorldActorEquipManager::new(&mut world);
        manager
            .unequip_object(
                soe_commonlib_shim::refr::ActorHandle(runtime_actor),
                FormId(runtime_item),
            )
            .unwrap();
    }
    assert!(
        !world
            .actor_ref(id(ACTOR))
            .unwrap()
            .equipped
            .contains(&id(ITEM))
    );
}

#[test]
fn shm_04_unmodeled_hooks_are_rejected_by_the_live_adapter() {
    let mut world = fixture();
    let manager = soe_game::commonlib::WorldActorEquipManager::new(&mut world);
    for hook in [
        HookError::AddressPatch,
        HookError::CallDetour,
        HookError::VtableOverride,
        HookError::Trampoline,
    ] {
        assert_eq!(manager.reject_hook(hook), Err(hook));
    }
}
