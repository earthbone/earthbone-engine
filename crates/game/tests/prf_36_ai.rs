// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRF-36 driver state and event ordering across worker counts, using PRF-12 records.

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
