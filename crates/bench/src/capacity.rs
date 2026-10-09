// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-28 capacity curves through existing engine readers and public APIs.
//! GMST names and types follow local UESP GMST and xEdit wbDefinitionsTES5.
//! Warm-up and separate setup follow godotwind's benchmarking manual.

mod animation;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use soe_esm::{GlobalFormId, LoadOrder, Tag};
use soe_game::{Game, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

const P99_MS: f64 = 16.7;
const WORST_MS: f64 = 33.0;

#[derive(Debug)]
struct Config {
    scenario: String,
    world: String,
    start: usize,
    max: usize,
    samples: usize,
    warmup: usize,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, String> {
        if args.len() < 2 || !(args.len() - 2).is_multiple_of(2) {
            return Err("usage: soe-bench capacity <ai|animation> <synthetic|whiterun> [--start N --max N --samples N --warmup N --workers 1 --level high]".into());
        }
        let mut config = Self {
            scenario: args[0].clone(),
            world: args[1].clone(),
            start: 16,
            max: 65_536,
            samples: 120,
            warmup: 10,
        };
        if !["ai", "animation"].contains(&config.scenario.as_str())
            || !["synthetic", "whiterun"].contains(&config.world.as_str())
        {
            return Err("unknown capacity scenario or world".into());
        }
        for pair in args[2..].as_chunks::<2>().0 {
            if pair[0] == "--level" {
                if config.scenario != "ai" || pair[1] != "high" {
                    return Err("only high-process AI executes in this checkout".into());
                }
                continue;
            }
            let value: usize = pair[1]
                .parse()
                .map_err(|_| format!("invalid {}", pair[0]))?;
            match pair[0].as_str() {
                "--start" => config.start = value,
                "--max" => config.max = value,
                "--samples" => config.samples = value,
                "--warmup" => config.warmup = value,
                "--workers" if config.scenario == "ai" && value == 1 => {}
                "--workers" => return Err("PRF-17 workers are absent from this checkout".into()),
                name => return Err(format!("unknown capacity option {name}")),
            }
        }
        if config.start == 0
            || config.max < config.start
            || config.max > 100_000
            || !(100..=100_000).contains(&config.samples)
            || !(1..=100_000).contains(&config.warmup)
        {
            return Err(
                "counts must be 1 to 100000, start <= max, samples >= 100, warmup >= 1".into(),
            );
        }
        Ok(config)
    }
}

fn data_dir() -> Result<PathBuf, String> {
    std::env::var_os("SKYRIM_DATA")
        .map(PathBuf::from)
        .ok_or_else(|| "SKYRIM_DATA is not set".into())
}

fn vanilla_order(data: &std::path::Path) -> Result<LoadOrder, String> {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", crate::read_plugin(data, "Skyrim.esm")?)
        .map_err(|e| e.to_string())?;
    Ok(order)
}

fn settings(order: &LoadOrder) -> Vec<String> {
    let mut names: Vec<_> = order
        .winners_of(T(b"GMST"))
        .filter_map(|(_, record)| record.editor_id())
        .filter(|name| {
            if !matches!(name.as_bytes().first(), Some(b'i' | b'u' | b'f')) {
                return false;
            }
            let lower = name.to_ascii_lowercase();
            ["actor", "process", "light", "shadow"]
                .iter()
                .any(|part| lower.contains(part))
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    names
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    if args == ["settings"] {
        let order = vanilla_order(&data_dir()?)?;
        let names = settings(&order);
        let world = World::new(order);
        for name in names {
            println!("GMST {name} {:?}", world.game_setting(&name));
        }
        for name in [
            "iAINumberActorsComplexScene",
            "iNumberActorsInCombatPlayer",
            "iNumberActorsAllowedToFollowPlayer",
        ] {
            println!("GMST_CANDIDATE {name} {:?}", world.game_setting(name));
        }
        return Ok(());
    }
    let config = Config::parse(args)?;
    println!("CAPABILITY ai_levels=high ai_workers=1 middle_high=unsupported middle_low=unsupported low=unsupported prf_17=absent");
    println!(
        "CAPACITY_BUDGET p99_ms={P99_MS} worst_ms={WORST_MS} samples={} warmup={}",
        config.samples, config.warmup
    );
    if config.scenario == "animation" {
        return animation::run(&config);
    }
    let order = if config.world == "whiterun" {
        Some(Arc::new(super::read_order_threads(&data_dir()?, 1)?))
    } else {
        None
    };
    sweep(&config, |count| {
        let (mut game, actors) = match &order {
            Some(order) => installed_actors(Arc::clone(order), count)?,
            None => super::stress::fixtures::actors(count)?,
        };
        for _ in 0..config.warmup {
            game.tick(1.0 / 60.0);
        }
        let mut samples = Vec::with_capacity(config.samples);
        for _ in 0..config.samples {
            let start = Instant::now();
            game.tick(1.0 / 60.0);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        verify_actors(&game, &actors)?;
        Ok(samples)
    })
}

fn verify_actors(game: &Game, actors: &[GlobalFormId]) -> Result<(), String> {
    let world = game.world();
    if world.ai.metrics.levels[0].actors != actors.len() as u64
        || world.ai.metrics.levels[1..]
            .iter()
            .any(|level| level.actors != 0)
    {
        return Err("AI process counts differ from requested work".into());
    }
    for actor in actors {
        let reference = world.reference_ref(*actor).ok_or("actor disappeared")?;
        if !reference.enabled || !reference.position.iter().all(|v| v.is_finite()) {
            return Err("actor disabled or position non-finite".into());
        }
        if world
            .ai
            .metrics
            .actors
            .get(actor)
            .is_none_or(|metrics| metrics.updates < 2)
        {
            return Err("actor did not execute repeatedly".into());
        }
    }
    Ok(())
}

fn installed_actors(
    order: Arc<LoadOrder>,
    count: usize,
) -> Result<(Game, Vec<GlobalFormId>), String> {
    let index = order.build_cell_index();
    let origin = index
        .cell_by_editor_id("WhiterunOrigin")
        .ok_or("WhiterunOrigin absent")?;
    let grid = index.grid_of(origin).ok_or("WhiterunOrigin has no grid")?;
    let mut cells: Vec<_> = index
        .cells()
        .map(|(cell, _)| cell)
        .filter(|cell| {
            index
                .grid_of(*cell)
                .is_some_and(|entry| entry.world == grid.world)
        })
        .collect();
    if let Some(cell) = index.world_persistent_cell(grid.world) {
        cells.push(cell);
    }
    cells.sort_unstable();
    cells.dedup();
    let mut templates = Vec::new();
    for cell in cells {
        for &id in index.references(cell) {
            let Some(record) = order
                .winner(id)
                .filter(|record| record.tag == T(b"ACHR") && record.flags & 0x800 == 0)
            else {
                continue;
            };
            let Some(base) = record.find(T(b"NAME")).and_then(|bytes| bytes.get(..4)) else {
                continue;
            };
            let raw = u32::from_le_bytes(base.try_into().map_err(|_| "invalid actor base")?);
            let owner = order.winner_ref(id).ok_or("missing actor owner")?.file;
            templates.push((order.resolve(owner, raw), cell, id));
        }
    }
    if templates.is_empty() {
        return Err("Whiterun has no enabled actor templates".into());
    }
    let game = Game::from_shared_order(order);
    let mut ids = Vec::with_capacity(count);
    for n in 0..count {
        let (base, cell, source) = templates[n % templates.len()];
        let position = game
            .world()
            .editor_position(source)
            .ok_or("Whiterun actor has no position")?;
        let id = game.world_mut().place_at(base, cell, false);
        {
            let mut world = game.world_mut();
            let reference = world.reference(id).ok_or("placed actor absent")?;
            reference.cell = Some(cell);
            reference.position = position;
            if !world.drive_actor(id) {
                return Err("cannot drive Whiterun actor".into());
            }
        }
        ids.push(id);
    }
    Ok((game, ids))
}

fn summarize(samples: &mut [f64]) -> Result<(f64, f64), String> {
    if samples.is_empty() || samples.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Err("invalid capacity samples".into());
    }
    samples.sort_by(f64::total_cmp);
    Ok((
        samples[(samples.len() * 99).div_ceil(100) - 1],
        samples[samples.len() - 1],
    ))
}

fn sweep(
    config: &Config,
    mut measure: impl FnMut(usize) -> Result<Vec<f64>, String>,
) -> Result<(), String> {
    let mut count = config.start;
    let mut maximum = 0;
    loop {
        let mut samples = measure(count)?;
        let (p99, worst) = summarize(&mut samples)?;
        let pass = p99 <= P99_MS && worst <= WORST_MS;
        println!("CAPACITY_SAMPLE {{\"scenario\":\"{}\",\"world\":\"{}\",\"load\":{count},\"workers\":1,\"samples\":{},\"p99_ms\":{p99:.6},\"worst_ms\":{worst:.6},\"pass\":{pass}}}", config.scenario, config.world, samples.len());
        if !pass || count == config.max {
            println!("CAPACITY_RESULT {{\"scenario\":\"{}\",\"world\":\"{}\",\"max_passing_rung\":{},\"first_failing_rung\":{},\"status\":\"{}\"}}", config.scenario, config.world, if pass { count } else { maximum }, if pass { "null".into() } else { count.to_string() }, if pass { "limit_not_found" } else { "boundary" });
            break;
        }
        maximum = count;
        count = count.saturating_mul(2).min(config.max);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }

    #[test]
    fn prf_28_unsupported_levels_and_workers_are_rejected() {
        for level in ["middle_high", "middle_low", "low"] {
            assert!(Config::parse(&args(&["ai", "synthetic", "--level", level])).is_err());
        }
        assert!(Config::parse(&args(&["ai", "synthetic", "--workers", "24"])).is_err());
    }

    #[test]
    fn prf_28_invalid_counts_and_samples_fail_before_setup() {
        for options in [
            ["--start", "0"],
            ["--samples", "99"],
            ["--max", "100001"],
            ["--warmup", "0"],
        ] {
            assert!(Config::parse(&args(&["ai", "synthetic", options[0], options[1]])).is_err());
        }
    }

    #[test]
    fn prf_28_curve_stops_at_first_budget_breach() {
        let config =
            Config::parse(&args(&["ai", "synthetic", "--start", "1", "--max", "8"])).unwrap();
        let mut counts = Vec::new();
        sweep(&config, |count| {
            counts.push(count);
            Ok(vec![if count < 4 { 1.0 } else { 34.0 }; 120])
        })
        .unwrap();
        assert_eq!(counts, [1, 2, 4]);
    }

    #[test]
    fn prf_28_curve_reports_unreached_limit_without_exceeding_max() {
        let config =
            Config::parse(&args(&["ai", "synthetic", "--start", "3", "--max", "7"])).unwrap();
        let mut counts = Vec::new();
        sweep(&config, |count| {
            counts.push(count);
            Ok(vec![1.0; 120])
        })
        .unwrap();
        assert_eq!(counts, [3, 6, 7]);
    }

    #[test]
    fn prf_28_percentiles_use_nearest_rank_and_reject_invalid_samples() {
        assert_eq!(
            summarize(&mut (1..=100).map(f64::from).collect::<Vec<_>>()).unwrap(),
            (99.0, 100.0)
        );
        assert!(summarize(&mut []).is_err());
        assert!(summarize(&mut [f64::NAN]).is_err());
    }

    #[test]
    fn prf_28_synthetic_high_process_count_matches_work() {
        let (mut game, actors) = super::super::stress::fixtures::actors(16).unwrap();
        for _ in 0..120 {
            game.tick(1.0 / 60.0);
        }
        verify_actors(&game, &actors).unwrap();
        assert!(actors.iter().step_by(2).all(|id| game
            .world()
            .reference_ref(*id)
            .unwrap()
            .position[0]
            > 10.0));
    }

    #[test]
    fn prf_28_numeric_setting_survey_retains_mod_overrides() {
        use soe_esm::{FileHeader, Plugin, Record};
        let mut order = LoadOrder::new();
        for (plugin_name, value) in [("Base.esm", 20i32), ("Raise.esp", 100)] {
            let mut plugin = Plugin::new(FileHeader::default());
            plugin.push_top(
                Record::new(T(b"GMST"), 0x800)
                    .zsub(T(b"EDID"), "iAINumberActorsComplexScene")
                    .sub(T(b"DATA"), value.to_le_bytes()),
            );
            order.add(plugin_name, plugin).unwrap();
        }
        assert_eq!(settings(&order), ["iAINumberActorsComplexScene"]);
        assert_eq!(
            World::new(order).game_setting("iAINumberActorsComplexScene"),
            Some(soe_papyrus::Value::Int(100))
        );
    }

    #[test]
    fn prf_28_installed_whiterun_actors_and_gmst_survey() {
        let Ok(data) = data_dir() else { return };
        let order = Arc::new(super::super::read_order_threads(&data, 1).unwrap());
        let names = settings(&order);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
        let world = World::from_shared_order(Arc::clone(&order));
        assert!(!names.is_empty());
        for name in &names {
            assert!(world.game_setting(name).is_some(), "stored GMST {name}");
        }
        drop(world);
        let (mut game, actors) = installed_actors(order, 8).unwrap();
        for _ in 0..3 {
            game.tick(1.0 / 60.0);
        }
        verify_actors(&game, &actors).unwrap();
    }
}
