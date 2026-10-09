// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-36 driver state and event ordering across worker counts, using PRF-12 records.

#[test]
fn prf_36_package_door_query_does_not_build_unrelated_reference_state() {
    let game = crate::transitions::game();
    let id = crate::transitions::id;
    let mut world = game.world_mut();
    assert_eq!(
        world.cell_doors(id(0x810)),
        vec![(id(0x820), Some(id(0x821))), (id(0x850), None)]
    );
    for object in [0x14, 0x830, 0x831, 0x832] {
        assert!(world.reference_ref(id(object)).is_none(), "{object:x}");
    }
    assert!(world.loaded_actors().is_empty());
}

#[test]
fn prf_36_alias_candidates_keep_priority_ties_and_observe_live_fills() {
    use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin};
    use soe_game::{ai, quest, World};

    let id = |object| GlobalFormId { file: 0, object };
    let actor = id(0x900);
    let mut plugin = Plugin::new(FileHeader::default());
    for object in [0x803, 0x802, 0x801] {
        plugin.push_top(quest::with_alias_packages(
            quest::build_record(object, "AliasQuest", 0),
            0,
            "Actor",
            Some(actor.object),
            &[object + 0x1000, object + 0x2000],
        ));
    }
    let mut order = LoadOrder::new();
    order.add("AliasPackages.esm", plugin).unwrap();
    let mut world = World::new(order);
    for (object, priority) in [(0x803, 10), (0x802, 20), (0x801, 20)] {
        let state = world.quest_state(id(object)).unwrap();
        state.running = true;
        state.def.priority = priority;
    }
    let expected = |quests: &[u32]| {
        quests
            .iter()
            .flat_map(|object| [object + 0x1000, object + 0x2000])
            .map(id)
            .collect::<Vec<_>>()
    };
    let packages = |world: &mut World| {
        ai::candidates(world, actor)
            .into_iter()
            .map(|candidate| candidate.package)
            .collect::<Vec<_>>()
    };
    assert_eq!(packages(&mut world), expected(&[0x801, 0x802, 0x803]));
    world.quest_state(id(0x801)).unwrap().running = false;
    world
        .quest_state(id(0x802))
        .unwrap()
        .aliases
        .insert(0, None);
    assert_eq!(packages(&mut world), expected(&[0x803]));
    world
        .quest_state(id(0x802))
        .unwrap()
        .aliases
        .insert(0, Some(actor));
    assert_eq!(packages(&mut world), expected(&[0x802, 0x803]));
}

#[test]
fn prf_36_driver_preserves_world_vm_and_work_counts_with_one_and_eight_workers() {
    let (mut single, _) = crate::prf_12_frame::actors(32).unwrap();
    let (mut parallel, _) = crate::prf_12_frame::actors(32).unwrap();
    let before = parallel.world().save_state();
    single
        .world_mut()
        .ai
        .set_planning_workers(1.try_into().unwrap())
        .unwrap();
    parallel
        .world_mut()
        .ai
        .set_planning_workers(8.try_into().unwrap())
        .unwrap();
    assert_eq!(parallel.world().save_state(), before);
    let mut jobs = 0;
    for frame in 0..120 {
        single.tick(1.0 / 60.0);
        parallel.tick(1.0 / 60.0);
        jobs += parallel.world().ai.movement_workers().jobs;
        assert_eq!(
            single.world().save_state(),
            parallel.world().save_state(),
            "frame {frame}"
        );
        assert_eq!(
            single.vm.save_state().unwrap(),
            parallel.vm.save_state().unwrap(),
            "frame {frame}"
        );
        assert_eq!(
            single.last_work_counts(),
            parallel.last_work_counts(),
            "frame {frame}"
        );
    }
    assert!(jobs > 0, "waypoint jobs executed on workers");
}
