// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! `soe-bench`: timings over the installed game, one line per measurement as
//! `name value unit` so two runs can be diffed. Every subcommand repeats its
//! work `--runs N` times (default 3) and reports median, minimum, p99 and maximum.
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
//! SKYRIM_DATA=<Data> soe-bench dirty
//! SKYRIM_DATA=<Data> soe-bench save
//! SKYRIM_DATA=<Data> soe-bench overhead WhiterunBanneredMare
//! SKYRIM_DATA=<Data> soe-bench open-data
//! SKYRIM_DATA=<Data> soe-bench trace cell WhiterunBanneredMare
//! SKYRIM_DATA=<Data> soe-bench all
//! ```

mod allocations;
mod gates;
mod profile_diff;
mod stress;

use gates::Gates;
use std::cell::RefCell;
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

const USAGE: &str = "usage: soe-bench [--runs N] [--out DIR] [--max-seconds metric=value] <load-order|open-data|archives|cell <editor id>|cell-index <editor id>|exterior <editor id>|scripts|navmesh|dirty|save [file.ess]|overhead <editor id>|trace <subcommand ...>|all>";

struct Options {
    gates: RefCell<Gates>,
    runs: usize,
    data: PathBuf,
    out: PathBuf,
    threads: usize,
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "profile-diff") {
        return match profile_diff::run(&args[1..]) {
            Ok(report) => {
                print!("{report}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    if args.first().is_some_and(|arg| arg == "stress") {
        return match stress::run(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("{error}");
                ExitCode::FAILURE
            }
        };
    }
    let mut runs = 3;
    let mut gates = Gates::default();
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
            Some("--max-seconds") if args.len() >= 2 => {
                if let Err(error) = gates.add(&args[1]) {
                    eprintln!("{error}");
                    return ExitCode::FAILURE;
                }
                args.drain(..2);
            }
            Some("--out") if args.len() >= 2 => {
                out = PathBuf::from(&args[1]);
                args.drain(..2);
            }
            _ => break,
        }
    }
    let data = std::env::var_os("SKYRIM_DATA").map_or_else(
        || {
            (args.first().is_some_and(|arg| arg == "save") && args.get(1).is_some())
                .then(PathBuf::new)
        },
        |path| Some(PathBuf::from(path)),
    );
    let Some(data) = data else {
        eprintln!("SKYRIM_DATA is not set");
        return ExitCode::FAILURE;
    };
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let opts = Options {
        gates: RefCell::new(gates),
        runs,
        data,
        out,
        threads,
    };
    report_count("machine_threads", threads, "threads");
    let result = dispatch(&opts, &args).and_then(|()| opts.gates.borrow().finish());
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
        Some("open-data") => open_data(opts),
        Some("archives") => archives(opts),
        Some("cell") => cell(opts, arg(1)?),
        Some("cell-index") => cell_index(opts, arg(1)?),
        Some("exterior") => exterior(opts, arg(1)?),
        Some("scripts") => scripts(opts),
        Some("navmesh") => navmesh(opts),
        Some("dirty") => dirty(opts),
        Some("save") => save(opts, args.get(1).map(String::as_str)),
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
            .and_then(|()| dirty(opts))
            .and_then(|()| save(opts, None))
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

/// Nearest-rank p99 and maximum. With fewer than 100 samples p99 is the max.
fn tail_summary(samples: &[Duration]) -> (f64, f64) {
    let mut sorted: Vec<_> = samples.iter().map(Duration::as_secs_f64).collect();
    sorted.sort_by(f64::total_cmp);
    (
        sorted[sorted.len() - sorted.len() / 100 - 1],
        sorted[sorted.len() - 1],
    )
}

/// Run `f` `runs` times, report durations under `name`, hand back the
/// median and the last result.
fn timed<T>(
    opts: &Options,
    name: &str,
    runs: usize,
    mut f: impl FnMut() -> Result<T, String>,
) -> Result<(f64, T), String> {
    let mut samples = Vec::with_capacity(runs);
    let mut last = None;
    for _ in 0..runs {
        // A result may own every archive. Release it before the next sample
        // to avoid retaining two complete data sources during construction.
        drop(last.take());
        let started = Instant::now();
        let value = f()?;
        samples.push(started.elapsed());
        last = Some(value);
    }
    let (median, min) = summary(&samples);
    report(&format!("{name}_median"), median, "s");
    report(&format!("{name}_min"), min, "s");
    let (p99, max) = tail_summary(&samples);
    for (suffix, value) in [("median", median), ("min", min), ("p99", p99), ("max", max)] {
        let metric = format!("{name}_{suffix}");
        if matches!(suffix, "p99" | "max") {
            report(&metric, value, "s");
        }
        opts.gates.borrow_mut().observe(&metric, value)?;
    }
    report_count(&format!("{name}_samples"), samples.len(), "samples");
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
    let (median_1, order) = timed(opts, "load_order_1_thread", opts.runs, || {
        read_order_threads(&opts.data, 1)
    })?;
    let name = format!("load_order_{}_threads", opts.threads);
    let (median_n, _) = timed(opts, &name, opts.runs, || {
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

fn open_data(opts: &Options) -> Result<(), String> {
    let cache = std::env::temp_dir().join(format!("soe-bench-plugin-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let cold = timed(opts, "open_data_cold", opts.runs, || {
        let _ = std::fs::remove_dir_all(&cache);
        soe_godot_host::data_source::DataSource::open_with_threads_and_cache(
            &opts.data,
            opts.threads,
            Some(&cache),
        )
        .map_err(|error| error.to_string())
    })?;
    drop(cold);
    let warm = timed(opts, "open_data_warm", opts.runs, || {
        soe_godot_host::data_source::DataSource::open_with_threads_and_cache(
            &opts.data,
            opts.threads,
            Some(&cache),
        )
        .map_err(|error| error.to_string())
    })?;
    drop(warm);
    std::fs::remove_dir_all(&cache).map_err(|error| format!("{}: {error}", cache.display()))?;
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
    let (index_median, vfs) = timed(opts, "archives_index", opts.runs, || {
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
    let (read_median, bytes) = timed(opts, "archives_read_1000", opts.runs, || {
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
    let (_, _) = timed(opts, "cell_open_data_1_thread", opts.runs, || {
        open_source(opts, 1)
    })?;
    let (_, mut source) = timed(
        opts,
        &format!("cell_open_data_{}_threads", opts.threads),
        opts.runs,
        || open_source(opts, opts.threads),
    )?;
    source.set_threads(1);
    let _ = timed(
        opts,
        &format!("cell_stream_{editor_id}_cold_1_thread"),
        1,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
    let (median_1, _) = timed(
        opts,
        &format!("cell_stream_{editor_id}_1_thread"),
        opts.runs,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
    source.set_threads(opts.threads);
    let (median_n, loaded) = timed(
        opts,
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
    let order = read_order_threads(&opts.data, opts.threads)?;
    let (median, ()) = timed(opts, "cell_index_build", opts.runs, || {
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
        opts,
        &format!("exterior_stream_3x3_{editor_id}_1_thread"),
        opts.runs,
        || stream_block(&source),
    )?;
    source.set_threads(opts.threads);
    let (median_n, totals) = timed(
        opts,
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
    let (_, load) = timed(opts, "scripts_load_from_bsa", opts.runs, || {
        let mut fresh = Game::new(read_order_threads(&opts.data, opts.threads)?);
        let report = fresh.load_scripts_from_data(&opts.data);
        game = fresh;
        Ok(report)
    })?;
    report_count("scripts_loaded", load.scripts_loaded, "scripts");
    report_count("scripts_failed", load.scripts_failed.len(), "scripts");
    let (_, bind) = timed(opts, "scripts_bind", 1, || Ok(game.bind()))?;
    report_count("scripts_attached", bind.scripts_attached, "instances");
    report_count("scripts_properties_set", bind.properties_set, "properties");
    let (_, started) = timed(opts, "scripts_start", 1, || Ok(game.start()))?;
    report_count("scripts_quests_started", started.quests_started, "quests");
    report_count(
        "scripts_live_threads_after_start",
        game.vm.live_threads(),
        "threads",
    );

    game.vm.load(hot_loop_script());
    let iterations = 200_000;
    let (loop_median, used) = timed(opts, "vm_hot_loop", opts.runs, || {
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

/// What xEdit's cleaning would touch in each master, counted by the esm
/// crate, one line per file and per kind.
fn dirty(opts: &Options) -> Result<(), String> {
    let order = read_order_threads(&opts.data, opts.threads)?;
    let (median, report_) = timed(opts, "dirty_report", opts.runs, || Ok(order.dirty_report()))?;
    let records: usize = order.files().iter().map(|f| f.plugin.records.len()).sum();
    report(
        "dirty_records_per_s",
        per_second(records, median),
        "records/s",
    );
    for p in &report_.plugins {
        let stem = p.name.trim_end_matches(".esm").trim_end_matches(".esp");
        report_count(
            &format!("dirty_{stem}_itm"),
            p.identical_to_master,
            "records",
        );
        report_count(
            &format!("dirty_{stem}_deleted_references"),
            p.deleted_references,
            "records",
        );
        report_count(
            &format!("dirty_{stem}_deleted_navmeshes"),
            p.deleted_navmeshes,
            "records",
        );
        report_count(
            &format!("dirty_{stem}_deleted_other"),
            p.deleted_other,
            "records",
        );
    }
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
    let (build_median, (graph, stats)) = timed(opts, "navmesh_build_tamriel", opts.runs, || {
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
    let (local_median, local_found) = timed(opts, "navmesh_1000_local_queries", opts.runs, || {
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
    let (_, _) = timed(opts, "navmesh_point_index_first_query", 1, || {
        Ok(graph.nearest(graph.centroid(local[0].0)))
    })?;
    let (_, located) = timed(opts, "navmesh_1000_point_queries", opts.runs, || {
        Ok(local
            .iter()
            .filter(|(a, _)| {
                let point = graph.centroid(*a);
                graph.locate(point).is_some() && graph.nearest(point).is_some()
            })
            .count())
    })?;
    report_count("navmesh_point_queries_found", located, "points");

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
    let (global_median, global_found) = timed(opts, "navmesh_100_province_queries", 1, || {
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

fn save(opts: &Options, input: Option<&str>) -> Result<(), String> {
    if let Some(input) = input {
        let path = Path::new(input);
        let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
        if bytes.starts_with(soe_game::save::MAGIC) {
            let mut game = Game::new(read_order_threads(&opts.data, opts.threads)?);
            game.load_scripts_from_data(&opts.data);
            game.bind();
            let (_, ()) = timed(opts, "save_load", opts.runs, || {
                game.load_state(&bytes).map_err(|error| error.to_string())
            })?;
            report_count("save_file_bytes", bytes.len(), "bytes");
            return Ok(());
        }
        let (_, save) = timed(opts, "save_parse", opts.runs, || {
            soe_save::Save::parse(&bytes).map_err(|error| error.to_string())
        })?;
        report_count("save_file_bytes", bytes.len(), "bytes");
        let mut global_sections = std::collections::BTreeMap::<u32, usize>::new();
        for (index, table) in [
            (&save.table_1, "table_1"),
            (&save.table_2, "table_2"),
            (&save.table_3, "table_3"),
        ] {
            report_count(
                &format!("save_{table}_bytes"),
                index.iter().map(|block| 8 + block.data.len()).sum(),
                "bytes",
            );
            for block in index {
                *global_sections.entry(block.kind).or_default() += 8 + block.data.len();
            }
        }
        for (kind, size) in global_sections {
            report_count(&format!("save_global_section_{kind}_bytes"), size, "bytes");
        }
        report_count("save_change_forms", save.change_forms.len(), "forms");
        for row in save.change_form_sizes_by_plugin() {
            println!(
                "save_plugin_change {} {} {} {}",
                row.plugin, row.record_type, row.count, row.bytes
            );
        }
        if let Some(Ok(papyrus)) = save.papyrus() {
            let bytes = save
                .table_3
                .iter()
                .find(|block| block.kind == soe_save::table::GlobalData::PAPYRUS)
                .map_or(0, |block| block.data.len());
            report_count("save_papyrus_bytes", bytes, "bytes");
            report_count("save_papyrus_scripts", papyrus.scripts.len(), "scripts");
            report_count(
                "save_papyrus_instances",
                papyrus.instances.len(),
                "instances",
            );
        }
        return Ok(());
    }
    let mut game = Game::new(read_order_threads(&opts.data, opts.threads)?);
    game.load_scripts_from_data(&opts.data);
    game.bind();
    let (_, bytes) = timed(opts, "save_state", opts.runs, || {
        game.save_state().map_err(|e| e.to_string())
    })?;
    report("save_size_mb", bytes.len() as f64 / 1e6, "MB");
    let mut fresh = Game::new(read_order_threads(&opts.data, opts.threads)?);
    fresh.load_scripts_from_data(&opts.data);
    let (_, ()) = timed(opts, "load_state", opts.runs, || {
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
        opts,
        &format!("overhead_{editor_id}_trace_off"),
        opts.runs,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
    Trace::clear();
    Trace::enable();
    let (on, _) = timed(
        opts,
        &format!("overhead_{editor_id}_trace_on"),
        opts.runs,
        || source.stream_cell(editor_id).map_err(|e| e.to_string()),
    )?;
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

#[cfg(test)]
mod timing_tests {
    use super::{tail_summary, timed, Gates, Options};
    use std::cell::{Cell, RefCell};
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn nearest_rank_uses_measured_samples_without_interpolation() {
        assert_eq!(tail_summary(&[Duration::from_secs(7)]), (7.0, 7.0));
        let samples: Vec<_> = (1..=100).rev().map(Duration::from_secs).collect();
        assert_eq!(tail_summary(&samples), (99.0, 100.0));
        assert_eq!(tail_summary(&samples[..3]), (100.0, 100.0));
    }

    #[test]
    fn samples_release_the_previous_result_before_starting_the_next() {
        struct Live<'a>(&'a Cell<usize>);
        impl Drop for Live<'_> {
            fn drop(&mut self) {
                self.0.set(self.0.get() - 1);
            }
        }
        let live = Cell::new(0);
        let options = Options {
            gates: RefCell::new(Gates::default()),
            runs: 3,
            data: PathBuf::default(),
            out: PathBuf::default(),
            threads: 1,
        };
        let (_, last) = timed(&options, "drop_test", 3, || {
            assert_eq!(live.get(), 0);
            live.set(1);
            Ok(Live(&live))
        })
        .unwrap();
        assert_eq!(live.get(), 1);
        drop(last);
        assert_eq!(live.get(), 0);
    }
}
