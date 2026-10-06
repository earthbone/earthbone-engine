// SPDX-License-Identifier: GPL-3.0-or-later

//! Fixed PRF-16 actor workload, using PRF-07's record-backed NAVM fixture and
//! PRF-12's exclusive system slots. Setup and reporting follow godotwind's
//! benchmarking manual. tools/frame_budget.py applies the documented budgets.

use super::{Config, Workload};
use soe_trace::{FrameSample, System};
use std::time::{Duration, Instant};

const FRAMES: usize = 3_600;

fn workload() -> Result<(Config, Workload), String> {
    let config = Config::parse(&["actors".into(), "--load".into(), "1000".into()])?;
    let mut workload = Workload::new(&config)?;
    {
        let mut world = workload.game.world_mut();
        let graph = soe_game::ai::nav_graph(&mut world);
        if graph.meshes().is_empty() || graph.meshes().iter().all(|mesh| mesh.triangles.is_empty())
        {
            return Err("thousand-actor scenario has no navmesh".into());
        }
    }
    for _ in 0..config.warmup {
        workload.step(&config)?;
    }
    Ok((config, workload))
}

fn stats(mut values: Vec<u64>) -> [f64; 3] {
    values.sort_unstable();
    [50, 99, 100].map(|rank| values[(values.len() * rank).div_ceil(100) - 1] as f64 / 1e6)
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("frame-budget takes no options, its workload is fixed".into());
    }
    let (config, mut workload) = workload()?;
    let mut samples: Vec<FrameSample> = Vec::with_capacity(FRAMES);
    let started = Instant::now();
    for _ in 0..FRAMES {
        workload.step(&config)?;
        samples.push(workload.game.frame_history().last().clone());
        let deadline = Duration::from_secs_f64(samples.len() as f64 / 60.0);
        std::thread::sleep(deadline.saturating_sub(started.elapsed()));
    }
    let elapsed = started.elapsed().as_secs_f64();
    workload.verify()?;
    let world = workload.game.world();
    let evaluations = world.ai.evaluations;
    if evaluations < 1_000
        || workload.actors.iter().any(|id| {
            world.actor_ref(*id).is_none_or(|actor| {
                !matches!(
                    actor.ai.procedure,
                    soe_game::ai::Procedure::Travel | soe_game::ai::Procedure::Sandbox
                )
            })
        })
    {
        return Err("not all thousand actors ran their packages".into());
    }
    let frame = stats(samples.iter().map(|sample| sample.frame_ns).collect());
    print!(
        "SOE_BUDGET {{\"scenario\":\"thousand-actors\",\"samples\":{FRAMES},\"game_ticks\":{FRAMES},\"elapsed_s\":{elapsed:.6},\"simulated_s\":60,\"actors\":1000,\"evaluations\":{evaluations},\"navmesh\":true,\"frame\":{{\"p50_ms\":{:.6},\"p99_ms\":{:.6},\"worst_ms\":{:.6}}},\"systems\":{{",
        frame[0], frame[1], frame[2]
    );
    for (index, system) in System::ALL.into_iter().enumerate() {
        let times = stats(
            samples
                .iter()
                .map(|s| s.system_ns[system as usize])
                .collect(),
        );
        print!(
            "{}\"{}\":{{\"p50_ms\":{:.6},\"p99_ms\":{:.6},\"worst_ms\":{:.6}}}",
            if index == 0 { "" } else { "," },
            system.name(),
            times[0],
            times[1],
            times[2]
        );
    }
    println!("}}}}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_16_thousand_actors_use_navmesh_packages_and_move() {
        let (config, mut workload) = workload().unwrap();
        for _ in 0..300 {
            workload.step(&config).unwrap();
        }
        workload.verify().unwrap();
        let world = workload.game.world();
        assert_eq!(workload.actors.len(), 1_000);
        assert!(workload.actors.iter().all(|id| world
            .actor_ref(*id)
            .is_some_and(|actor| actor.ai.procedure != soe_game::ai::Procedure::None)));
        assert!(world.ai.evaluations >= 1_000);
        assert!(
            workload
                .game
                .frame_history()
                .last()
                .milliseconds(System::AiPlanning)
                > 0.0
        );
    }

    #[test]
    fn prf_16_percentiles_keep_early_hitches_and_idle_samples() {
        let mut samples = vec![0; FRAMES];
        samples[0] = 33_000_001;
        assert_eq!(stats(samples), [0.0, 0.0, 33.000_001]);
        assert_eq!(stats(vec![1_000_000, 2_000_000]), [1.0, 2.0, 2.0]);
    }

    #[test]
    fn prf_16_fixed_workload_rejects_overrides() {
        assert!(run(&["--samples".into(), "1".into()]).is_err());
    }
}
