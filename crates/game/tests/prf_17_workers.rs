// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Navigation worker integration using PRF-12's record-backed stress fixture.
//! Worker ownership and source boundaries are recorded in docs/threading.md.

use soe_game::Game;

#[test]
fn prf_17_stress_world_matches_with_one_and_eight_workers_without_saved_configuration() {
    let (mut single, _) = crate::prf_12_frame::actors(1000).unwrap();
    let (mut parallel, _) = crate::prf_12_frame::actors(1000).unwrap();
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
    let mut times = [[0u64; 2]; 2];
    let mut jobs = [0; 2];
    for frame in 0..32 {
        single.tick(1.0 / 60.0);
        parallel.tick(1.0 / 60.0);
        for (index, game) in [&single, &parallel].into_iter().enumerate() {
            let sample = game.frame_history().last();
            times[index][0] += sample.system_ns[soe_trace::System::AiPlanning as usize];
            times[index][1] += sample.system_ns[soe_trace::System::Papyrus as usize];
            jobs[index] += game.world().ai.planning_workers().jobs;
        }
        assert_eq!(
            single.last_work_counts(),
            parallel.last_work_counts(),
            "frame {frame}"
        );
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
    }
    eprintln!("PRF-17 PRF-12 stress, 1000 actors, 32 ticks: workers=[1,8] ai_vm_ns={times:?} navigation_jobs={jobs:?}");
}

#[test]
fn prf_17_vanilla_bannered_mare_matches_with_one_and_eight_workers() {
    let Some(order) = crate::common::installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let mut single = Game::from_shared_order(order.clone());
    let mut parallel = Game::from_shared_order(order);
    for (game, workers) in [(&mut single, 1usize), (&mut parallel, 8usize)] {
        game.world_mut()
            .ai
            .set_planning_workers(workers.try_into().unwrap())
            .unwrap();
        let cell = game
            .world_mut()
            .cell_by_editor_id("WhiterunBanneredMare")
            .unwrap();
        game.load_cell(cell);
        let references = game.world_mut().cell_references(cell);
        for id in references {
            if game
                .world()
                .record(id)
                .is_some_and(|record| record.tag == soe_esm::tag::ACHR)
            {
                game.world_mut().actor(id);
            }
        }
        assert!(game.world().loaded_actors().len() >= 5);
    }
    for frame in 0..120 {
        single.tick(1.0 / 60.0);
        parallel.tick(1.0 / 60.0);
        assert_eq!(
            single.last_work_counts(),
            parallel.last_work_counts(),
            "frame {frame}"
        );
    }
    assert_eq!(single.world().save_state(), parallel.world().save_state());
    assert_eq!(
        single.vm.save_state().unwrap(),
        parallel.vm.save_state().unwrap()
    );
}
