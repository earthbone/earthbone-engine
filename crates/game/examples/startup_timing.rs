// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-33 quest startup timing, adapted from the lead's 2026-10-03 host harness.
//! Reads the same five masters and script overlay without opening rendering assets.
//! Pass --trace to record phases inside World::start_quest.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use soe_esm::{LoadOrder, Plugin, Tag};
use soe_game::Game;
use soe_trace::Trace;

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = PathBuf::from(std::env::var("SKYRIM_DATA")?);
    let begin = Instant::now();
    let mut order = LoadOrder::new();
    let mut vfs = soe_bsa::Vfs::new();
    let mut archives: Vec<_> = std::fs::read_dir(&data)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    archives.sort();
    for name in ["Skyrim", "Update", "Dawnguard", "HearthFires", "Dragonborn"] {
        let plugin_name = format!("{name}.esm");
        order.add(
            &plugin_name,
            Plugin::parse(&std::fs::read(data.join(&plugin_name))?)?,
        )?;
        let stem = name.to_ascii_lowercase();
        let wanted = [format!("{stem}.bsa"), format!("{stem} - misc.bsa")];
        for path in &archives {
            if path
                .file_name()
                .is_some_and(|name| wanted.contains(&name.to_string_lossy().to_lowercase()))
            {
                vfs.add_archive_path(path)?;
            }
        }
    }
    vfs.add_loose_root(&data)?;
    println!("open_masters_and_script_vfs_ms={:.3}", ms(begin.elapsed()));

    let begin = Instant::now();
    let mut game = Game::from_shared_order(Arc::new(order));
    println!("game_from_order_ms={:.3}", ms(begin.elapsed()));
    let begin = Instant::now();
    let mut scripts: Vec<_> = vfs
        .list("scripts")
        .into_iter()
        .filter(|path| {
            Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pex"))
        })
        .collect();
    scripts.sort_unstable();
    println!(
        "list_scripts_ms={:.3} count={}",
        ms(begin.elapsed()),
        scripts.len()
    );
    let (mut read, mut parse, mut total_bytes) = (Duration::ZERO, Duration::ZERO, 0usize);
    for path in scripts {
        let begin = Instant::now();
        let bytes = vfs.read(path)?;
        read += begin.elapsed();
        total_bytes += bytes.len();
        let begin = Instant::now();
        game.load_script(&bytes)?;
        parse += begin.elapsed();
    }
    println!(
        "read_scripts_ms={:.3} parse_and_register_ms={:.3} bytes={total_bytes}",
        ms(read),
        ms(parse)
    );
    let begin = Instant::now();
    let report = game.bind();
    println!(
        "bind_ms={:.3} attached={} missing={}",
        ms(begin.elapsed()),
        report.scripts_attached,
        report.scripts_missing.len()
    );

    let quests: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"QUST"))
        .map(|(id, _)| id)
        .collect();
    if std::env::args().any(|arg| arg == "--trace") {
        Trace::clear();
        Trace::enable();
    }
    let begin = Instant::now();
    let (mut attempted, mut started) = (0, 0);
    let mut per_quest = Vec::new();
    for id in quests {
        if !game
            .world_mut()
            .quest_state(id)
            .is_some_and(|quest| quest.def.starts_enabled())
        {
            continue;
        }
        attempted += 1;
        let begin = Instant::now();
        started += usize::from(game.start_quest(id));
        let elapsed = begin.elapsed();
        let name = game
            .world()
            .record(id)
            .and_then(|record| record.editor_id())
            .unwrap_or_default();
        per_quest.push((elapsed, name));
    }
    let total = begin.elapsed();
    Trace::disable();
    println!(
        "start_quests_ms={:.3} attempted={attempted} started={started}",
        ms(total)
    );
    per_quest.sort_unstable();
    if let (Some(median), Some(slowest)) = (per_quest.get(per_quest.len() / 2), per_quest.last()) {
        let rank = (per_quest.len() * 99).div_ceil(100).saturating_sub(1);
        let p99 = per_quest.get(rank).map_or(Duration::ZERO, |quest| quest.0);
        println!(
            "per_quest_ms p50={:.6} p99={:.6} max={:.6}",
            ms(median.0),
            ms(p99),
            ms(slowest.0)
        );
    }
    for (elapsed, name) in per_quest.iter().rev().take(12) {
        println!("slow_quest_ms={:.6} name={name}", ms(*elapsed));
    }
    for (elapsed, name) in &per_quest {
        if [
            "FreeformRiften07",
            "PlayerVampireQuest",
            "CreatureDialogueHorse",
            "DialogueSolitude",
        ]
        .contains(&name.as_str())
        {
            println!("sample_quest_ms={:.6} name={name}", ms(*elapsed));
        }
    }
    println!("{}", Trace::summary_text());
    println!("startup_world_hash={:?}", game.world().state_hash());
    let begin = Instant::now();
    let idle = game.run_until_idle(64);
    println!("run_until_idle_ms={:.3} idle={idle}", ms(begin.elapsed()));
    println!("settled_world_hash={:?}", game.world().state_hash());
    Ok(())
}
