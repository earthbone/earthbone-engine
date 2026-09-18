// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! `soe-bench`: timings over the installed game, one line per measurement as
//! `name value unit` so two runs can be diffed. Every subcommand repeats its
//! work `--runs N` times (default 3) and reports the median and the minimum.
//! Phases that have a threaded path report the one-thread and the
//! all-threads wall time side by side. `trace <subcommand>` records every
//! span (`soe-trace`) and writes a Chrome trace file plus a text summary.
//! Numbers land in `docs/performance.md`.
//!
//! ```text
//! SKYRIM_DATA=<Data> soe-bench [--runs N] [--out DIR] load-order
//! SKYRIM_DATA=<Data> soe-bench archives
//! SKYRIM_DATA=<Data> soe-bench cell WhiterunBanneredMare
//! SKYRIM_DATA=<Data> soe-bench cell-index WhiterunBanneredMare
//! SKYRIM_DATA=<Data> soe-bench exterior WhiterunOrigin
//! SKYRIM_DATA=<Data> soe-bench scripts
//! SKYRIM_DATA=<Data> soe-bench navmesh
//! SKYRIM_DATA=<Data> soe-bench save
//! SKYRIM_DATA=<Data> soe-bench overhead WhiterunBanneredMare
//! SKYRIM_DATA=<Data> soe-bench trace cell WhiterunBanneredMare
//! SKYRIM_DATA=<Data> soe-bench all
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use soe_esm::{tag, GlobalFormId, LoadOrder, Plugin};
use soe_game::Game;
use soe_godot_host::data_source::DataSource;
use soe_navmesh::{vanilla, NodeId};
use soe_papyrus::asm::{i, ident, none, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{Assign, CmpLt, IAdd, Jmp, JmpF, Return};
use soe_papyrus::Value;
use soe_trace::Trace;

const MASTERS: [&str; 5] = [
    "Skyrim.esm",
    "Update.esm",
    "Dawnguard.esm",
    "HearthFires.esm",
    "Dragonborn.esm",
];

const USAGE: &str = "usage: soe-bench [--runs N] [--out DIR] <load-order|archives|cell <editor id>|cell-index <editor id>|exterior <editor id>|scripts|navmesh|save|overhead <editor id>|trace <subcommand ...>|all>";

struct Options {
    runs: usize,
    data: PathBuf,
    out: PathBuf,
    threads: usize,
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut runs = 3;
    let mut out = PathBuf::from(".");
    loop {
        match args.first().map(String::as_str) {
            Some("--runs") if args.len() >= 2 => match args[1].parse() {
                Ok(n) if n > 0 => {
                    runs = n;
                    args.drain(..2);
                }
                _ => {
                    eprintln!("{USAGE}");
                    return ExitCode::FAILURE;
                }
            },
            Some("--out") if args.len() >= 2 => {
                out = PathBuf::from(&args[1]);
                args.drain(..2);
            }
            _ => break,
        }
    }
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA is not set");
        return ExitCode::FAILURE;
    };
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let opts = Options {
        runs,
        data,
        out,
        threads,
    };
    report_count("machine_threads", threads, "threads");
    let result = dispatch(&opts, &args);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(opts: &Options, args: &[String]) -> Result<(), String> {
    let arg = |k: usize| {
        args.get(k)
            .map(String::as_str)
            .ok_or_else(|| USAGE.to_string())
    };
    match args.first().map(String::as_str) {
        Some("load-order") => load_order(opts),
        Some("archives") => archives(opts),
        Some("cell") => cell(opts, arg(1)?),
        Some("cell-index") => cell_index(opts, arg(1)?),
        Some("exterior") => exterior(opts, arg(1)?),
        Some("scripts") => scripts(opts),
        Some("navmesh") => navmesh(opts),
        Some("save") => save(opts),
        Some("overhead") => overhead(opts, arg(1)?),
        Some("trace") => {
            if args.len() < 2 {
                return Err(USAGE.to_string());
            }
            Trace::enable();
            Trace::clear();
            let result = dispatch(opts, &args[1..]);
            Trace::disable();
            let stem = format!("soe-bench-trace-{}", args[1..].join("-"));
            let json = opts.out.join(format!("{stem}.json"));
            let text = opts.out.join(format!("{stem}.txt"));
            Trace::export_chrome_json(&json).map_err(|e| format!("{}: {e}", json.display()))?;
            let summary = Trace::summary_text();
            std::fs::write(&text, &summary).map_err(|e| format!("{}: {e}", text.display()))?;
            report_count("trace_events", Trace::events().len(), "events");
            println!("trace_json {}", json.display());
            println!("trace_summary {}", text.display());
            print!("{summary}");
            Trace::clear();
            result
        }
        Some("all") => load_order(opts)
            .and_then(|()| archives(opts))
            .and_then(|()| cell(opts, "WhiterunBanneredMare"))
            .and_then(|()| cell_index(opts, "WhiterunBanneredMare"))
            .and_then(|()| exterior(opts, "WhiterunOrigin"))
            .and_then(|()| scripts(opts))
            .and_then(|()| navmesh(opts))
            .and_then(|()| save(opts))
            .and_then(|()| overhead(opts, "WhiterunBanneredMare")),
        _ => Err(USAGE.to_string()),
    }
}

// ----- reporting -----

fn report(name: &str, value: f64, unit: &str) {
    println!("{name} {value:.3} {unit}");
}

fn report_count(name: &str, value: usize, unit: &str) {
    println!("{name} {value} {unit}");
}

/// Median and minimum of the samples, in seconds.
fn summary(samples: &[Duration]) -> (f64, f64) {
    let mut secs: Vec<f64> = samples.iter().map(Duration::as_secs_f64).collect();
    secs.sort_by(f64::total_cmp);
    let median = secs[secs.len() / 2];
    let min = secs[0];
    (median, min)
}

/// Run `f` `runs` times, report median and min under `name`, hand back the
/// median and the last result.
fn timed<T>(
    name: &str,
    runs: usize,
    mut f: impl FnMut() -> Result<T, String>,
) -> Result<(f64, T), String> {
    let mut samples = Vec::with_capacity(runs);
    let mut last = None;
    for _ in 0..runs {
        let started = Instant::now();
        let value = f()?;
        samples.push(started.elapsed());
        last = Some(value);
    }
    let (median, min) = summary(&samples);
    report(&format!("{name}_median"), median, "s");
    report(&format!("{name}_min"), min, "s");
    let value = last.ok_or_else(|| "no runs".to_string())?;
    Ok((median, value))
}

fn per_second(count: usize, seconds: f64) -> f64 {
    if seconds > 0.0 {
        count as f64 / seconds
    } else {
        0.0
    }
}

// ----- shared -----

fn read_plugin(data: &Path, name: &str) -> Result<Plugin, String> {
    let path = data.join(name);
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    Plugin::parse(&bytes).map_err(|e| format!("{name}: {e}"))
}

/// The vanilla masters as a load order. With more than one thread the
/// files parse in parallel and join in master order.
fn read_order_threads(data: &Path, threads: usize) -> Result<LoadOrder, String> {
    let _span = soe_trace::Span::with_payload("bench.read_order", "bench", threads as u64);
    let plugins: Vec<Result<Plugin, String>> = if threads <= 1 {
        MASTERS.iter().map(|name| read_plugin(data, name)).collect()
    } else {
        std::thread::scope(|scope| {
            let handles: Vec<_> = MASTERS
                .iter()
                .map(|name| scope.spawn(move || read_plugin(data, name)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err("a parse thread panicked".to_string()))
                })
                .collect()
        })
    };
    let mut order = LoadOrder::new();
    for (name, plugin) in MASTERS.iter().zip(plugins) {
        order
            .add(name, plugin?)
            .map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(order)
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

// ----- subcommands -----

fn load_order(opts: &Options) -> Result<(), String> {
    let (median_1, order) = timed("load_order_1_thread", opts.runs, || {
        read_order_threads(&opts.data, 1)
    })?;
    let name = format!("load_order_{}_threads", opts.threads);
    let (median_n, _) = timed(&name, opts.runs, || {
        read_order_threads(&opts.data, opts.threads)
    })?;
    report_count("load_order_forms", order.len(), "forms");
    report(
        "load_order_forms_per_s_1_thread",
        per_second(order.len(), median_1),
        "forms/s",
    );
    report(
        "load_order_forms_per_s_threads",
        per_second(order.len(), median_n),
        "forms/s",
    );
    report(
        "load_order_thread_speedup",
        median_1 / median_n.max(1e-9),
        "x",
    );
    Ok(())
}

fn archives(opts: &Options) -> Result<(), String> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&opts.data)
        .map_err(|e| format!("{}: {e}", opts.data.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("bsa")))
        .collect();
    paths.sort();
    let (index_median, vfs) = timed("archives_index", opts.runs, || {
        let mut vfs = soe_bsa::Vfs::new();
        for p in &paths {
            vfs.add_archive_path(p)
                .map_err(|e| format!("{}: {e}", p.display()))?;
        }
        Ok(vfs)
    })?;
    report_count("archives_count", paths.len(), "archives");
    report_count("archives_files", vfs.len(), "files");
    report(
        "archives_files_indexed_per_s",
        per_second(vfs.len(), index_median),
        "files/s",
    );

    let all: Vec<&str> = vfs.paths().collect();
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let sample: Vec<&str> = (0..1000).map(|_| all[rng.below(all.len())]).collect();
    let (read_median, bytes) = timed("archives_read_1000", opts.runs, || {
        let mut total = 0usize;
        for p in &sample {
            total += vfs.read(p).map_err(|e| format!("{p}: {e}"))?.len();
        }
        Ok(total)
    })?;
    report("archives_read_mb", bytes as f64 / 1e6, "MB");
    report(
        "archives_read_mb_per_s",
        bytes as f64 / 1e6 / read_median.max(1e-9),
        "MB/s",
    );
    Ok(())
}

fn open_source(opts: &Options, threads: usize) -> Result<DataSource, String> {
    DataSource::open_with_threads(&opts.data, threads).map_err(|e| e.to_string())
}

fn cell(opts: &Options, editor_id: &str) -> Result<(), String> {
    let (_, _) = timed("cell_open_data_1_thread", opts.runs, || {
        open_source(opts, 1)
    })?;
    let (_, mut source) = timed(
        &format!("cell_open_data_{}_threads", opts.threads),
        opts.runs,
        || open_source(opts, opts.threads),
    )?;
    source.set_threads(1);
    let (median_1, _) = timed(
        &format!("cell_stream_{editor_id}_1_thread"),
        opts.runs,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
    source.set_threads(opts.threads);
    let (median_n, loaded) = timed(
        &format!("cell_stream_{editor_id}_{}_threads", opts.threads),
        opts.runs,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
    let s = &loaded.stats;
    report_count("cell_references", s.references, "refs");
    report_count("cell_statics_with_model", s.statics_with_model, "statics");
    report_count("cell_meshes_parsed", s.meshes_parsed, "meshes");
    report_count("cell_meshes_failed", s.meshes_failed, "meshes");
    report_count("cell_triangles", s.triangles, "triangles");
    report_count("cell_surfaces", loaded.surfaces.len(), "surfaces");
    report(
        "cell_triangles_per_s_1_thread",
        per_second(s.triangles, median_1),
        "triangles/s",
    );
    report(
        "cell_triangles_per_s_threads",
        per_second(s.triangles, median_n),
        "triangles/s",
    );
    report("cell_thread_speedup", median_1 / median_n.max(1e-9), "x");
    Ok(())
}

/// The cell index over the vanilla masters: build time on top of a parsed
/// order, what it holds, and the cost of one lookup.
fn cell_index(opts: &Options, editor_id: &str) -> Result<(), String> {
    let mut order = read_order_threads(&opts.data, opts.threads)?;
    let (median, ()) = timed("cell_index_build", opts.runs, || {
        order.build_cell_index();
        Ok(())
    })?;
    let index = order
        .cell_index()
        .ok_or_else(|| "the index was not kept".to_string())?;
    report_count("cell_index_cells", index.cell_count(), "cells");
    report_count("cell_index_placed", index.placed_count(), "objects");
    report_count("cell_index_grid_cells", index.grid_count(), "cells");
    report_count("cell_index_worldspaces", index.world_count(), "worldspaces");
    report(
        "cell_index_objects_per_s",
        per_second(index.placed_count(), median),
        "objects/s",
    );
    let cell = index
        .cell_by_editor_id(editor_id)
        .ok_or_else(|| format!("{editor_id}: not in the index"))?;
    report_count(
        &format!("cell_index_{editor_id}_objects"),
        index.references(cell).len(),
        "objects",
    );
    let lookups = 1_000_000usize;
    let started = Instant::now();
    let mut total = 0usize;
    for _ in 0..lookups {
        total += index.references(cell).len();
    }
    let elapsed = started.elapsed().as_secs_f64();
    report("cell_index_lookup", elapsed * 1e9 / lookups as f64, "ns");
    report_count("cell_index_lookup_ids_summed", total, "ids");
    Ok(())
}

/// The 3 by 3 block of exterior cells around the named cell, by grid
/// position in the same worldspace, through the index the data source
/// built at open.
fn exterior(opts: &Options, editor_id: &str) -> Result<(), String> {
    let mut source = open_source(opts, opts.threads)?;
    let index = source.cell_index();
    let origin = index
        .cell_by_editor_id(editor_id)
        .ok_or_else(|| format!("{editor_id}: no cell with that editor id"))?;
    let grid = index.grid_of(origin).ok_or_else(|| {
        format!("{editor_id}: not an exterior cell, exteriors need a cell in a worldspace with an XCLC grid")
    })?;
    let mut block: Vec<(i32, i32, GlobalFormId)> = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (x, y) = (grid.x + dx, grid.y + dy);
            if let Some(id) = index.exterior_cell(grid.world, x, y) {
                block.push((x, y, id));
            }
        }
    }
    report_count("exterior_cells_in_block", block.len(), "cells");
    let stream_block = |source: &DataSource| -> Result<(usize, usize, usize, usize), String> {
        let mut totals = (0, 0, 0, 0);
        for (x, y, id) in &block {
            let loaded = source
                .stream_cell_id(*id)
                .map_err(|e| format!("cell ({x}, {y}) {id}: {e}"))?;
            totals.0 += loaded.stats.references;
            totals.1 += loaded.stats.meshes_parsed;
            totals.2 += loaded.stats.meshes_failed;
            totals.3 += loaded.stats.triangles;
        }
        Ok(totals)
    };
    source.set_threads(1);
    let (median_1, _) = timed(
        &format!("exterior_stream_3x3_{editor_id}_1_thread"),
        opts.runs,
        || stream_block(&source),
    )?;
    source.set_threads(opts.threads);
    let (median_n, totals) = timed(
        &format!("exterior_stream_3x3_{editor_id}_{}_threads", opts.threads),
        opts.runs,
        || stream_block(&source),
    )?;
    report_count("exterior_references", totals.0, "refs");
    report_count("exterior_meshes_parsed", totals.1, "meshes");
    report_count("exterior_meshes_failed", totals.2, "meshes");
    report_count("exterior_triangles", totals.3, "triangles");
    report(
        "exterior_triangles_per_s_1_thread",
        per_second(totals.3, median_1),
        "triangles/s",
    );
    report(
        "exterior_triangles_per_s_threads",
        per_second(totals.3, median_n),
        "triangles/s",
    );
    report(
        "exterior_thread_speedup",
        median_1 / median_n.max(1e-9),
        "x",
    );
    Ok(())
}

/// A counting loop the VM runs until `n` is reached: four instructions per
/// iteration.
fn hot_loop_script() -> soe_papyrus::Script {
    ScriptBuilder::new("SoeBench", "")
        .function(
            FunctionBuilder::new("Loop", "None")
                .global()
                .param("n", "Int")
                .local("i", "Int")
                .local("t", "Bool")
                .ins(Assign, vec![ident("i"), i(0)])
                .ins(CmpLt, vec![ident("t"), ident("i"), ident("n")])
                .ins(JmpF, vec![ident("t"), i(3)])
                .ins(IAdd, vec![ident("i"), ident("i"), i(1)])
                .ins(Jmp, vec![i(-3)])
                .ins(Return, vec![none()])
                .build(),
        )
        .build()
}

/// The VM is single threaded by contract, so every number here is one
/// thread.
fn scripts(opts: &Options) -> Result<(), String> {
    let order = read_order_threads(&opts.data, opts.threads)?;
    let mut game = Game::new(order);
    let (_, load) = timed("scripts_load_from_bsa", opts.runs, || {
        let mut fresh = Game::new(read_order_threads(&opts.data, opts.threads)?);
        let report = fresh.load_scripts_from_data(&opts.data);
        game = fresh;
        Ok(report)
    })?;
    report_count("scripts_loaded", load.scripts_loaded, "scripts");
    report_count("scripts_failed", load.scripts_failed.len(), "scripts");
    let (_, bind) = timed("scripts_bind", 1, || Ok(game.bind()))?;
    report_count("scripts_attached", bind.scripts_attached, "instances");
    report_count("scripts_properties_set", bind.properties_set, "properties");
    let (_, started) = timed("scripts_start", 1, || Ok(game.start()))?;
    report_count("scripts_quests_started", started.quests_started, "quests");
    report_count(
        "scripts_live_threads_after_start",
        game.vm.live_threads(),
        "threads",
    );

    game.vm.load(hot_loop_script());
    let iterations = 200_000;
    let (loop_median, used) = timed("vm_hot_loop", opts.runs, || {
        game.vm
            .call_global("SoeBench", "Loop", &[Value::Int(iterations)])
            .map_err(|e| e.to_string())?;
        let mut used = 0;
        while !game.vm.idle() {
            used += game.vm.run(1_000_000);
        }
        Ok(used)
    })?;
    report_count("vm_hot_loop_instructions", used, "instructions");
    report(
        "vm_instructions_per_s",
        per_second(used, loop_median),
        "instructions/s",
    );
    Ok(())
}

fn navmesh(opts: &Options) -> Result<(), String> {
    let order = read_order_threads(&opts.data, opts.threads)?;
    let tamriel = order
        .winners_of(tag::WRLD)
        .find(|(_, r)| r.editor_id().as_deref() == Some("Tamriel"))
        .map(|(id, _)| id)
        .ok_or_else(|| "Tamriel not found".to_string())?;
    let tamriel_raw = order.record(order.overrides(tamriel)[0]).form_id;
    let (build_median, (graph, stats)) = timed("navmesh_build_tamriel", opts.runs, || {
        Ok(vanilla::from_load_order(&order, |r| {
            r.placement.world == Some(tamriel_raw)
        }))
    })?;
    report_count("navmesh_meshes", stats.meshes, "meshes");
    report_count("navmesh_triangles", stats.triangles, "triangles");
    report(
        "navmesh_triangles_built_per_s",
        per_second(stats.triangles, build_median),
        "triangles/s",
    );

    let meshes = graph.meshes();
    let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
    // Local queries: both ends inside one navmesh, the shape of an AI
    // package moving an actor around a cell.
    let local: Vec<(NodeId, NodeId)> = (0..1000)
        .map(|_| {
            let m = rng.below(meshes.len());
            let n = meshes[m].triangles.len();
            (
                NodeId::new(m as u32, rng.below(n) as u32),
                NodeId::new(m as u32, rng.below(n) as u32),
            )
        })
        .collect();
    let (local_median, local_found) = timed("navmesh_1000_local_queries", opts.runs, || {
        Ok(local
            .iter()
            .filter(|(a, b)| graph.find_path(*a, *b).is_some())
            .count())
    })?;
    report_count("navmesh_local_paths_found", local_found, "paths");
    report(
        "navmesh_local_queries_per_s",
        per_second(local.len(), local_median),
        "queries/s",
    );
    // Random queries across the whole province: half the pairs are not
    // connected and the search exhausts its component before giving up.
    let count = 100;
    let global: Vec<(NodeId, NodeId)> = (0..count)
        .map(|_| {
            let pick = |rng: &mut XorShift| {
                let m = rng.below(meshes.len());
                let t = rng.below(meshes[m].triangles.len());
                NodeId::new(m as u32, t as u32)
            };
            (pick(&mut rng), pick(&mut rng))
        })
        .collect();
    let (global_median, global_found) = timed("navmesh_100_province_queries", 1, || {
        Ok(global
            .iter()
            .filter(|(a, b)| graph.find_path(*a, *b).is_some())
            .count())
    })?;
    report_count("navmesh_province_paths_found", global_found, "paths");
    report(
        "navmesh_province_queries_per_s",
        per_second(count, global_median),
        "queries/s",
    );
    Ok(())
}

fn save(opts: &Options) -> Result<(), String> {
    let mut game = Game::new(read_order_threads(&opts.data, opts.threads)?);
    game.load_scripts_from_data(&opts.data);
    game.bind();
    let (_, bytes) = timed("save_state", opts.runs, || {
        game.save_state().map_err(|e| e.to_string())
    })?;
    report("save_size_mb", bytes.len() as f64 / 1e6, "MB");
    let mut fresh = Game::new(read_order_threads(&opts.data, opts.threads)?);
    fresh.load_scripts_from_data(&opts.data);
    let (_, ()) = timed("load_state", opts.runs, || {
        fresh.load_state(&bytes).map_err(|e| e.to_string())
    })?;
    Ok(())
}

/// The cost of the spans themselves: the same cell streamed with tracing
/// off and on.
fn overhead(opts: &Options, editor_id: &str) -> Result<(), String> {
    let was_enabled = Trace::enabled();
    let source = open_source(opts, opts.threads)?;
    Trace::disable();
    let (off, _) = timed(
        &format!("overhead_{editor_id}_trace_off"),
        opts.runs,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
    Trace::clear();
    Trace::enable();
    let (on, _) = timed(&format!("overhead_{editor_id}_trace_on"), opts.runs, || {
        source.stream_cell(editor_id).map_err(|e| e.to_string())
    })?;
    let events = Trace::events().len();
    Trace::clear();
    if !was_enabled {
        Trace::disable();
    }
    report_count(
        "overhead_events_per_run",
        events / opts.runs.max(1),
        "events",
    );
    report("overhead_ratio", on / off.max(1e-9), "x");
    report(
        "overhead_per_event_ns",
        (on - off).max(0.0) * 1e9 / (events / opts.runs.max(1)).max(1) as f64,
        "ns",
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hot_loop_runs_the_expected_instruction_count() {
        let mut vm = soe_papyrus::Vm::new();
        vm.load(hot_loop_script());
        vm.call_global("SoeBench", "Loop", &[Value::Int(10)])
            .unwrap();
        let mut used = 0;
        while !vm.idle() {
            used += vm.run(10_000);
        }
        assert!(vm.log.entries.is_empty(), "{:?}", vm.log.messages());
        // One assign, ten iterations of four instructions, a final compare
        // and jump, and the return.
        assert_eq!(used, 1 + 10 * 4 + 2 + 1);
    }

    #[test]
    fn summary_reports_median_and_min() {
        let s = [
            Duration::from_millis(30),
            Duration::from_millis(10),
            Duration::from_millis(20),
        ];
        let (median, min) = summary(&s);
        assert!((median - 0.020).abs() < 1e-9);
        assert!((min - 0.010).abs() < 1e-9);
    }
}
