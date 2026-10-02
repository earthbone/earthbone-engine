// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::form::FormId;
use soe_commonlib_shim::world::{
    ActivateParentID, ActorCriticalStage, ActorValueModifier, UnsupportedState, WorldState,
};
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{shim_world::CommonLibWorld, Game};
use soe_papyrus::ObjectId;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn shm_03_live_actor_and_ai_state_cross_the_mit_api() {
    let mut game = Game::new(LoadOrder::new());
    let actor = game.player();
    let actor_id = FormId(0x14);
    {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.0).unwrap();
        world.mod_actor_value(actor, "health", 7.0).unwrap();
        world
            .mod_temporary_actor_value(actor, "health", 5.0)
            .unwrap();
        world.damage_actor_value(actor, "health", 3.0).unwrap();
        world.actor(actor).unwrap().ai.procedure = soe_game::ai::Procedure::Travel;
    }
    {
        let mut live = game.world_mut();
        let mut api = CommonLibWorld::new(&mut live);
        assert_eq!(
            api.critical_stage(actor_id).unwrap(),
            ActorCriticalStage::None
        );
        api.set_critical_stage(actor_id, ActorCriticalStage::GooStart)
            .unwrap();
        assert_eq!(
            api.critical_stage(actor_id).unwrap(),
            ActorCriticalStage::GooStart
        );
        assert_eq!(
            api.actor_value_modifier(actor_id, 24, ActorValueModifier::Permanent)
                .unwrap(),
            7.0
        );
        assert_eq!(
            api.actor_value_modifier(actor_id, 24, ActorValueModifier::Temporary)
                .unwrap(),
            5.0
        );
        assert_eq!(
            api.actor_value_modifier(actor_id, 24, ActorValueModifier::Damage)
                .unwrap(),
            -3.0
        );
        assert_eq!(api.ai_process(actor_id).unwrap().procedure, "Travel");
        assert!(matches!(
            api.unsupported_hook("AIProcess::GetCharController"),
            Err(UnsupportedState::Hook(_))
        ));
        assert!(matches!(
            api.active_effect(999),
            Err(UnsupportedState::MissingActiveEffect)
        ));
    }
    assert_eq!(game.world_mut().actor(actor).unwrap().critical_stage, 1);
}

#[test]
fn shm_03_activate_parent_ids_resolve_from_live_reference_records() {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut parent = Vec::from(0x901u32.to_le_bytes());
    parent.extend_from_slice(&2.5f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"REFR"), 0x900)
            .sub(T(b"NAME"), 0x902u32.to_le_bytes())
            .sub(T(b"XAPR"), parent),
    );
    plugin.push_top(Record::new(T(b"REFR"), 0x901).sub(T(b"NAME"), 0x902u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    let game = Game::new(order);
    let mut live = game.world_mut();
    let mut api = CommonLibWorld::new(&mut live);
    assert_eq!(
        api.activate_parents(FormId(0x900)).unwrap(),
        vec![ActivateParentID {
            ref_id: FormId(0x901),
            activate_delay: 2.5,
        }]
    );
}

#[test]
fn shm_03_active_effect_view_reads_live_application_state() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), 0x500).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), 0x100)
            .sub(T(b"NAME"), 0x500u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut effect_data = vec![0; 152];
    effect_data[0..4].copy_from_slice(&(1u32 << 1).to_le_bytes());
    effect_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect_data[64..68].copy_from_slice(&0u32.to_le_bytes());
    effect_data[68..72].copy_from_slice(&24u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), 0x200).sub(T(b"DATA"), effect_data));
    let mut spell_data = vec![0; 36];
    spell_data[8..12].copy_from_slice(&0u32.to_le_bytes());
    let mut efit = 12.0f32.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&5u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), 0x300)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), 0x200u32.to_le_bytes())
            .sub(T(b"EFIT"), efit),
    );
    let mut order = LoadOrder::new();
    order.add("Shim.esm", plugin).unwrap();
    let game = Game::new(order);
    let source = id(0x100);
    let shared = game.shared_world();
    let effect_ids = shared
        .borrow_mut()
        .cast_spell(
            id(0x300),
            source,
            Some(source),
            Some(source),
            &mut |_, _| ObjectId(0),
        )
        .unwrap();
    let mut live = game.world_mut();
    let api = CommonLibWorld::new(&mut live);
    let effect = api.active_effect(effect_ids[0].0).unwrap();
    assert_eq!(effect.spell, FormId(0x300));
    assert_eq!(effect.effect, FormId(0x200));
    assert_eq!(effect.target, FormId(0x100));
    assert_eq!(effect.magnitude, 12.0);
}
