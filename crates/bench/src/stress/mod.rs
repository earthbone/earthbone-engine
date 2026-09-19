// SPDX-License-Identifier: GPL-3.0-or-later

//! Capacity probes through public engine APIs. Workload ladders, warm-up and
//! separate resource observations follow engine-refs/godotwind's benchmarking
//! manual. The Python runner owns process isolation, budgets and refinement.

mod fixtures;

use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use soe_esm::{GlobalFormId, LoadOrder};
use soe_game::Game;
use soe_godot_host::mesh::{build_surface, MeshData, MeshOptions, SurfaceArrays};
use soe_godot_host::streaming::{CellCoord, CellStreamer, StreamConfig};
use soe_godot_host::units::{SkyrimPos, CELL_UNITS};
use soe_navmesh::NavGraph;
use soe_papyrus::ObjectId;

#[derive(Debug)]
struct Config {
    scenario: String,
    load: usize,
    samples: usize,
    warmup: usize,
    seed: u64,
    seconds: u64,
    vm_budget: usize,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut config = Self {
            scenario: args
                .first()
                .ok_or("stress needs a scenario: scripts, actors, navigation, streaming, mixed")?
                .clone(),
            load: 1,
            samples: 120,
            warmup: 10,
            seed: 1,
            seconds: 0,
            vm_budget: 100_000,
        };
        if !["scripts", "actors", "navigation", "streaming", "mixed"]
            .contains(&config.scenario.as_str())
        {
            return Err("unknown stress scenario".into());
        }
        if !(args.len() - 1).is_multiple_of(2) {
            return Err("stress options need values".into());
        }
        for pair in args[1..].chunks_exact(2) {
            let value: usize = pair[1]
                .parse()
                .map_err(|_| format!("invalid {}", pair[0]))?;
            match pair[0].as_str() {
                "--load" => config.load = value,
                "--samples" => config.samples = value,
                "--warmup" => config.warmup = value,
                "--seed" => config.seed = value as u64,
                "--seconds" => config.seconds = value as u64,
                "--vm-budget" => config.vm_budget = value,
                other => return Err(format!("unknown stress option {other}")),
            }
        }
        if !(1..=100_000).contains(&config.load)
            || !(1..=100_000).contains(&config.samples)
            || config.warmup > 100_000
            || config.seconds > 86_400
            || config.vm_budget == 0
        {
            return Err("load and samples must be 1 to 100000, warmup at most 100000, seconds at most 86400, VM budget positive".into());
        }
        Ok(config)
    }

    fn includes(&self, scenario: &str) -> bool {
        self.scenario == scenario || self.scenario == "mixed"
    }
}

struct Workload {
    game: Game,
    actors: Vec<GlobalFormId>,
    scripts: Vec<ObjectId>,
    graph: NavGraph,
    streamer: CellStreamer,
    mesh: MeshData,
    cells: HashMap<CellCoord, Vec<SurfaceArrays>>,
    tick: u64,
    work: u64,
}

impl Workload {
    fn new(config: &Config) -> Result<Self, String> {
        let (mut game, actors) = if config.includes("actors") {
            fixtures::actors(config.load)?
        } else {
            (Game::new(LoadOrder::new()), vec![])
        };
        game.budget = config.vm_budget;
        let scripts = fixtures::scripts(
            &mut game.vm,
            if config.includes("scripts") {
                config.load
            } else {
                0
            },
        )?;
        Ok(Self {
            game,
            actors,
            scripts,
            graph: fixtures::navigation(),
            streamer: CellStreamer::new(StreamConfig {
                max_loads_per_tick: 0,
                max_unloads_per_tick: 0,
                ..StreamConfig::default()
            })
            .map_err(|e| e.to_string())?,
            mesh: fixtures::mesh(config.seed),
            cells: HashMap::new(),
            tick: 0,
            work: 0,
        })
    }

    fn step(&mut self, config: &Config) -> Result<(), String> {
        for &object in &self.scripts {
            self.game
                .vm
                .call(object, "Update", &[])
                .map_err(|e| e.to_string())?;
        }
        if config.includes("actors") {
            self.game.tick(1.0 / 60.0);
        } else if config.includes("scripts") {
            self.game.vm.advance(1.0 / 60.0);
            self.game.vm.run(config.vm_budget);
        }
        if config.includes("navigation") {
            for q in 0..config.load {
                self.work += fixtures::query(&self.graph, config.seed, self.tick, q)? as u64;
            }
        }
        if config.includes("streaming") {
            // A bounded back-and-forth route revisits allocations during a soak.
            let x = (self.tick % 64).min(64 - self.tick % 64) as f32;
            let plan = self.streamer.update(SkyrimPos::new(
                (x + 0.5) * CELL_UNITS,
                0.5 * CELL_UNITS,
                0.0,
            ));
            for cell in plan.unload {
                self.cells.remove(&cell);
            }
            for cell in plan.load {
                let surfaces: Result<Vec<_>, _> = (0..config.load)
                    .map(|_| {
                        build_surface(&self.mesh, MeshOptions::default()).map_err(|e| e.to_string())
                    })
                    .collect();
                self.cells.insert(cell, surfaces?);
                self.work += config.load as u64 * 64;
            }
        }
        self.tick += 1;
        black_box(&self.cells);
        Ok(())
    }

    fn verify(&self) -> Result<u64, String> {
        if !self.game.vm.log.entries.is_empty() {
            return Err("stress VM logged errors".into());
        }
        let world = self.game.world();
        for &actor in &self.actors {
            let reference = world
                .reference_ref(actor)
                .ok_or("stress actor disappeared")?;
            if reference.position[0] <= 10.0 || !reference.position.iter().all(|x| x.is_finite()) {
                return Err(format!(
                    "actor {actor} did not move along its travel package"
                ));
            }
        }
        if self.cells.len() != self.streamer.resident_count() || self.cells.len() > 49 {
            return Err("streaming residency escaped its bounded ring".into());
        }
        let completed: u64 = self
            .scripts
            .iter()
            .map(|&object| {
                self.game
                    .vm
                    .get_var(object, "Stress", "completed")
                    .map_or(0, |v| v.as_int() as u64)
            })
            .sum();
        let issued = self.tick * self.scripts.len() as u64;
        let pending = self.game.vm.live_threads() as u64;
        if completed > issued || completed + pending < issued {
            return Err("script arrivals disappeared without completing".into());
        }
        Ok(completed)
    }
}

fn percentile(sorted: &[f64], numerator: usize, denominator: usize) -> f64 {
    sorted[(sorted.len() * numerator)
        .div_ceil(denominator)
        .saturating_sub(1)]
}

fn rss_mib() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("VmRSS:")?;
        Some(rest.split_whitespace().next()?.parse::<f64>().ok()? / 1024.0)
    })
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let config = Config::parse(args)?;
    let setup = Instant::now();
    let mut workload = Workload::new(&config)?;
    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
    for _ in 0..config.warmup {
        workload.step(&config)?;
    }
    let started = Instant::now();
    let mut window = 0;
    loop {
        let mut samples = Vec::with_capacity(config.samples);
        let mut backlog_peak = workload.game.vm.live_threads();
        let mut telemetry_ms = 0.0;
        for _ in 0..config.samples {
            let tick = Instant::now();
            workload.step(&config)?;
            samples.push(tick.elapsed().as_secs_f64() * 1000.0);
            let probe = Instant::now();
            backlog_peak = backlog_peak.max(workload.game.vm.live_threads());
            telemetry_ms += probe.elapsed().as_secs_f64() * 1000.0;
            if config.seconds > 0 {
                std::thread::sleep(
                    Duration::from_secs_f64(1.0 / 60.0).saturating_sub(tick.elapsed()),
                );
                if started.elapsed().as_secs() >= config.seconds {
                    break;
                }
            }
        }
        let completed = workload.verify()?;
        samples.sort_by(f64::total_cmp);
        let rss = rss_mib().map_or_else(|| "null".into(), |v| format!("{v:.6}"));
        println!("SOE_SAMPLE {{\"window\":{window},\"load\":{},\"samples\":{},\"ticks\":{},\"elapsed_s\":{:.6},\"setup_ms\":{setup_ms:.6},\"p50_ms\":{:.6},\"p99_ms\":{:.6},\"p999_ms\":{:.6},\"worst_ms\":{:.6},\"backlog\":{},\"backlog_peak\":{backlog_peak},\"completed\":{completed},\"actors\":{},\"resident_cells\":{},\"work_units\":{},\"rss_mib\":{rss},\"telemetry_ms\":{telemetry_ms:.6}}}",
            config.load, samples.len(), workload.tick, started.elapsed().as_secs_f64(),
            percentile(&samples, 50, 100), percentile(&samples, 99, 100), percentile(&samples, 999, 1000),
            samples[samples.len()-1], workload.game.vm.live_threads(), workload.actors.len(), workload.cells.len(), workload.work);
        window += 1;
        if config.seconds == 0 || started.elapsed().as_secs() >= config.seconds {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(scenario: &str, load: usize) -> Config {
        Config::parse(&[scenario.into(), "--load".into(), load.to_string()]).unwrap()
    }

    #[test]
    fn mixed_workload_moves_actors_completes_scripts_paths_and_streams() {
        let config = config("mixed", 4);
        let mut workload = Workload::new(&config).unwrap();
        for _ in 0..5 {
            workload.step(&config).unwrap();
        }
        assert_eq!(workload.verify().unwrap(), 20);
        assert_eq!(workload.game.vm.live_threads(), 0);
        assert!(workload.work > 0);
        assert!(workload.cells.len() >= 25);
    }

    #[test]
    fn overload_exposes_unfinished_work_instead_of_reporting_only_fast_ticks() {
        let mut config = config("scripts", 10);
        config.vm_budget = 1;
        let mut workload = Workload::new(&config).unwrap();
        for _ in 0..3 {
            workload.step(&config).unwrap();
        }
        assert_eq!(workload.game.vm.live_threads(), 30);
        assert_eq!(workload.verify().unwrap(), 0);
    }

    #[test]
    fn revisiting_cells_releases_surfaces_and_keeps_residency_bounded() {
        let config = config("streaming", 2);
        let mut workload = Workload::new(&config).unwrap();
        for _ in 0..130 {
            workload.step(&config).unwrap();
            workload.verify().unwrap();
        }
        assert!(workload.cells.len() <= 49);
    }

    #[test]
    fn invalid_workloads_fail_before_allocation() {
        for args in [
            vec!["bad"],
            vec!["mixed", "--load", "0"],
            vec!["scripts", "--samples", "100001"],
            vec!["actors", "--typo", "1"],
            vec!["mixed", "--load"],
        ] {
            assert!(
                Config::parse(&args.into_iter().map(str::to_string).collect::<Vec<_>>()).is_err()
            );
        }
    }
}
