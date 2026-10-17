// SPDX-License-Identifier: GPL-3.0-or-later

//! Waypoint steering isolated from package effects. This preserves the existing
//! PRF-11 driver budget, arrival tolerance and per-waypoint cell ownership.
//! OpenMW AiPackage::pathTo uses ground-plane progress for walking actors.
//! Only the actor's detached AI state is borrowed by the PRF-17 worker pool.

use soe_esm::GlobalFormId;

use super::{distance, AiState, Procedure, ARRIVAL_DISTANCE};

pub(super) fn follow(
    ai: &mut AiState,
    position: [f32; 3],
    cell: Option<GlobalFormId>,
    dt: f32,
) -> ([f32; 3], Option<GlobalFormId>) {
    let mut pos = position;
    let mut new_cell = cell;
    let mut budget = ai.speed * dt;
    while budget > 0.0 && !ai.path.is_empty() {
        let next = ai.path[0];
        // CONTEXT: Native NAVM planes differ from the collision floor. The host
        // resolves gravity, so a walking actor cannot reach a buried waypoint.
        let settings = soe_physics::CharacterSettings::mudcrab();
        let d = if (next[2] - pos[2]).abs() <= settings.step_height + settings.ground_snap {
            (next[0] - pos[0]).hypot(next[1] - pos[1])
        } else {
            distance(pos, next)
        };
        if ai.procedure == Procedure::Patrol
            && ai.path.len() == 1
            && ai.radius > 0.0
            && d - ai.radius <= budget
        {
            let t = ((d - ai.radius) / d).clamp(0.0, 1.0);
            pos = std::array::from_fn(|axis| pos[axis] + (next[axis] - pos[axis]) * t);
            if let Some(Some(target_cell)) = ai.path_cells.first() {
                new_cell = Some(*target_cell);
            }
            ai.path.clear();
            ai.path_cells.clear();
            ai.arrived = true;
            break;
        }
        let waypoint_budget = if ai.procedure == Procedure::Patrol {
            budget
        } else {
            budget.max(ARRIVAL_DISTANCE)
        };
        if d <= waypoint_budget {
            pos = next;
            budget -= d;
            ai.path.remove(0);
            if !ai.path_cells.is_empty() {
                if let Some(c) = ai.path_cells.remove(0) {
                    new_cell = Some(c);
                }
            }
        } else {
            let t = budget / d;
            pos = [
                pos[0] + (next[0] - pos[0]) * t,
                pos[1] + (next[1] - pos[1]) * t,
                pos[2] + (next[2] - pos[2]) * t,
            ];
            budget = 0.0;
        }
    }
    (pos, new_cell)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{navigation::Planner, AiState};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    #[test]
    fn grounded_route_advances_past_navmesh_points_below_the_collision_floor() {
        let mut ai = AiState {
            procedure: Procedure::Sandbox,
            speed: 80.1,
            path: vec![[10.0, 0.0, 0.0], [100.0, 0.0, 0.0]],
            path_cells: vec![Some(id(1)), Some(id(1))],
            ..AiState::default()
        };
        let mut position = [0.0, 0.0, 8.0];
        for _ in 0..60 {
            position = follow(&mut ai, position, Some(id(1)), 1.0 / 60.0).0;
            position[2] = 8.0;
        }
        assert!(
            position[0] >= 75.0,
            "route pinned to a NAVM height below the floor: {position:?}"
        );
        assert_eq!(ai.path, [[100.0, 0.0, 0.0]]);
    }

    #[test]
    fn grounded_route_keeps_a_waypoint_on_a_different_floor() {
        let mut ai = AiState {
            procedure: Procedure::Travel,
            speed: 80.1,
            path: vec![[0.0, 0.0, 120.0], [100.0, 0.0, 120.0]],
            ..AiState::default()
        };
        let position = follow(&mut ai, [0.0, 0.0, 8.0], None, 1.0 / 60.0).0;
        assert_eq!(ai.path.len(), 2);
        assert!(position[2] < 10.0);
    }

    #[test]
    fn waypoint_following_without_a_pool_matches_serial_without_worker_jobs() {
        let mut planner = Planner::default();
        let mut ai = AiState {
            procedure: Procedure::Travel,
            speed: 100.0,
            path: vec![[10.0, 0.0, 0.0], [100.0, 0.0, 0.0]],
            path_cells: vec![Some(id(1)), Some(id(2))],
            ..AiState::default()
        };
        let mut serial = ai.clone();
        let expected = follow(&mut serial, [0.0; 3], None, 0.25);
        assert_eq!(
            planner.follow(&mut ai, [0.0; 3], None, 0.25, None),
            expected
        );
        assert_eq!(ai, serial);
        assert_eq!(planner.movement_workers.jobs, 0);
        assert!(planner.movement_workers.busy.is_empty());
        assert!(planner.movement_workers.phase_busy.is_empty());
        assert!(planner.movement_workers.main_thread_wait.is_zero());
    }

    #[test]
    fn prf_36_worker_waypoints_match_serial_and_retain_cell_crossings() {
        for workers in [1, 8] {
            let mut planner = Planner::default();
            planner.set_workers(workers.try_into().unwrap()).unwrap();
            let mut ai = AiState {
                procedure: Procedure::Travel,
                speed: 100.0,
                path: vec![[10.0, 0.0, 0.0], [20.0, 0.0, 0.0], [100.0, 0.0, 0.0]],
                path_cells: vec![Some(id(1)), Some(id(2)), Some(id(3))],
                ..AiState::default()
            };
            let mut serial = ai.clone();
            let expected = follow(&mut serial, [0.0; 3], None, 0.25);
            let actual = planner.follow(&mut ai, [0.0; 3], None, 0.25, None);
            assert_eq!(actual, ([25.0, 0.0, 0.0], Some(id(2))));
            assert_eq!(actual, expected);
            assert_eq!(ai, serial);
            assert_eq!(planner.movement_workers.jobs, 1);
            assert!(planner
                .movement_workers
                .busy
                .iter()
                .any(|busy| !busy.is_zero()));
            assert!(
                planner.movement_workers.main_thread_wait
                    >= planner.movement_workers.phase_busy["path_following"]
            );
        }
    }

    #[test]
    fn prf_36_worker_patrol_stops_at_radius_and_preserves_arrival_tolerance() {
        let mut planner = Planner::default();
        let mut ai = AiState {
            procedure: Procedure::Patrol,
            speed: 100.0,
            radius: 5.0,
            path: vec![[20.0, 0.0, 0.0]],
            path_cells: vec![Some(id(2))],
            ..AiState::default()
        };
        assert_eq!(
            planner.follow(&mut ai, [0.0; 3], None, 0.2, None),
            ([15.0, 0.0, 0.0], Some(id(2)))
        );
        assert!(ai.arrived);
        assert!(ai.path.is_empty());
        ai.procedure = Procedure::Travel;
        ai.path = vec![[3.0, 0.0, 0.0]];
        assert_eq!(
            planner.follow(&mut ai, [0.0; 3], None, 0.001, None).0,
            [3.0, 0.0, 0.0]
        );
    }

    #[test]
    fn prf_36_worker_wait_stays_in_main_thread_time_and_busy_is_separate() {
        let mut planner = Planner::default();
        planner.set_workers(1.try_into().unwrap()).unwrap();
        let mut ai = AiState {
            speed: 100.0,
            path: vec![[100.0, 0.0, 0.0]],
            ..AiState::default()
        };
        let snapshot = soe_trace::FrameSnapshot::now();
        {
            let _scope = soe_trace::SystemScope::new(soe_trace::System::AiMovement);
            planner.follow(&mut ai, [0.0; 3], None, 0.1, None);
        }
        let sample = snapshot.finish();
        assert!(
            u128::from(sample.system_ns[soe_trace::System::AiMovement as usize])
                >= planner.movement_workers.main_thread_wait.as_nanos()
        );
        assert_eq!(sample.system_ns[soe_trace::System::AiPlanning as usize], 0);
        assert!(!planner.movement_workers.phase_busy["path_following"].is_zero());
    }
}
