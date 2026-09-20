// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Runs the reader over real Special Edition saves.
//!
//! Saves are user data and none of them is in this repository. The corpus the
//! user supplied sits outside it, under `engine-refs/saves/`, and these tests
//! skip cleanly when it is absent. `SOE_TEST_SAVE` names one file to use
//! instead, and `SOE_TEST_SAVES` names a directory to walk.
//!
//! The corpus is two sets. "Clean Saves" is nineteen files named after the
//! main quest stage each follows. The second set is forty one files numbered
//! in play order across the main quest, the Dawnguard vampire path and
//! Dragonborn. The numbering is what makes the second set the stronger
//! fixture: a sequence with a known order supports a progression check that no
//! single file can give, since a stage marked done in one save must still be
//! done in every later one.

use std::path::{Path, PathBuf};

use soe_save::Save;

const CORPUS: &str =
    "/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs/saves";

/// Every save the corpus holds, sorted so a numbered set is in play order.
fn corpus() -> Vec<PathBuf> {
    if let Some(one) = std::env::var_os("SOE_TEST_SAVE") {
        return vec![PathBuf::from(one)];
    }
    let root =
        std::env::var_os("SOE_TEST_SAVES").map_or_else(|| PathBuf::from(CORPUS), PathBuf::from);
    let mut found = Vec::new();
    collect(&root, &mut found);
    found.sort();
    found
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "ess") {
            out.push(path);
        }
    }
}

fn name_of(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[test]
fn every_save_in_the_corpus_parses() {
    let saves = corpus();
    if saves.is_empty() {
        eprintln!("no saves found, set SOE_TEST_SAVE or SOE_TEST_SAVES, skipping");
        return;
    }

    let mut failures = Vec::new();
    let mut total_change_forms = 0usize;
    let mut total_stages = 0usize;
    let mut total_values = 0usize;
    let mut total_arrays = 0usize;
    let mut total_value_bytes = 0usize;
    let mut total_execution_bytes = 0usize;
    let mut total_frames = 0usize;
    let mut total_instructions = 0usize;
    let mut total_threads = 0usize;
    let mut total_suspended = 0usize;
    let mut total_messages = 0usize;

    println!(
        "{:<48} {:>5} {:>7} {:>8} {:>7} {:>7} {:>9}",
        "save", "lvl", "plugins", "changes", "quests", "globals", "papyrus"
    );

    for path in &saves {
        let save = match Save::open(path) {
            Ok(save) => save,
            Err(error) => {
                failures.push(format!("{}: {error}", name_of(path)));
                continue;
            }
        };

        let stages = match save.quest_stages() {
            Ok(stages) => stages,
            Err(error) => {
                failures.push(format!("{}: quest stages: {error}", name_of(path)));
                continue;
            }
        };
        let globals = match save.globals() {
            Ok(globals) => globals.variables.len(),
            Err(error) => {
                failures.push(format!("{}: globals: {error}", name_of(path)));
                continue;
            }
        };
        let papyrus = match save.papyrus() {
            Some(Ok(block)) => {
                assert_eq!(block.instances.len(), block.instance_data.len());
                assert_eq!(block.references.len(), block.reference_data.len());
                assert_eq!(block.arrays.len(), block.array_data.len());
                let raw = &save
                    .table_3
                    .iter()
                    .find(|data| data.kind == 1001)
                    .unwrap()
                    .data;
                let rewritten = block.value_data_bytes().expect("typed values encode");
                assert_eq!(
                    rewritten,
                    raw[block.data_start..block.execution_start],
                    "{}: typed Papyrus values changed",
                    name_of(path)
                );
                let execution = block.execution.to_bytes().expect("typed execution encodes");
                assert_eq!(
                    execution,
                    raw[block.execution_start..block.decoded_len],
                    "{}: typed Papyrus execution changed",
                    name_of(path)
                );
                assert_eq!(
                    block.active_scripts.len(),
                    block.execution.active_data.len()
                );
                total_execution_bytes += execution.len();
                total_threads += block.execution.active_data.len();
                total_messages += block.execution.messages.len();
                total_suspended += block
                    .execution
                    .suspended
                    .iter()
                    .map(Vec::len)
                    .sum::<usize>();
                for active in &block.execution.active_data {
                    total_frames += active.frames.len();
                    total_instructions += active
                        .frames
                        .iter()
                        .map(|frame| frame.instructions.len())
                        .sum::<usize>();
                }
                let values = block
                    .instance_data
                    .iter()
                    .chain(&block.reference_data)
                    .map(|data| data.variables.len())
                    .sum::<usize>()
                    + block
                        .array_data
                        .iter()
                        .map(|data| data.variables.len())
                        .sum::<usize>();
                assert!(values > 0, "{}: no Papyrus values decoded", name_of(path));
                total_values += values;
                total_arrays += block.arrays.len();
                total_value_bytes += rewritten.len();
                block.block_len
            }
            Some(Err(error)) => {
                failures.push(format!("{}: papyrus: {error}", name_of(path)));
                0
            }
            None => {
                failures.push(format!("{}: no Papyrus block", name_of(path)));
                0
            }
        };

        total_change_forms += save.change_forms.len();
        total_stages += stages.iter().map(|(_, s)| s.len()).sum::<usize>();

        println!(
            "{:<48} {:>5} {:>7} {:>8} {:>7} {:>7} {:>9}",
            truncate(&name_of(path), 48),
            save.header.player_level,
            save.plugins.len() + save.light_plugins.len(),
            save.change_forms.len(),
            stages.len(),
            globals,
            papyrus
        );
    }

    println!(
        "\n{} saves, {total_change_forms} change forms, {total_stages} quest stage entries",
        saves.len()
    );
    println!("{total_values} Papyrus values, {total_arrays} arrays, {total_value_bytes} typed payload bytes round tripped");
    println!("{total_threads} threads, {total_frames} frames, {total_instructions} instructions, {total_messages} messages, {total_suspended} suspended stacks, {total_execution_bytes} execution bytes round tripped");

    assert!(
        failures.is_empty(),
        "{} of {} saves failed:\n{}",
        failures.len(),
        saves.len(),
        failures.join("\n")
    );
}

fn truncate(text: &str, width: usize) -> String {
    if text.len() <= width {
        text.to_string()
    } else {
        text.chars().take(width).collect()
    }
}

#[test]
fn every_save_body_round_trips_byte_for_byte() {
    let saves = corpus();
    if saves.is_empty() {
        eprintln!("no saves found, skipping");
        return;
    }

    let mut failures = Vec::new();
    for path in &saves {
        let Ok(bytes) = std::fs::read(path) else {
            failures.push(format!("{}: unreadable", name_of(path)));
            continue;
        };
        let (_header, _len, body) = match Save::split(&bytes) {
            Ok(split) => split,
            Err(error) => {
                failures.push(format!("{}: {error}", name_of(path)));
                continue;
            }
        };
        let save = match Save::parse(&bytes) {
            Ok(save) => save,
            Err(error) => {
                failures.push(format!("{}: {error}", name_of(path)));
                continue;
            }
        };
        let rewritten = save.body_bytes();
        if rewritten != body {
            let at = rewritten
                .iter()
                .zip(&body)
                .position(|(a, b)| a != b)
                .unwrap_or(rewritten.len().min(body.len()));
            failures.push(format!(
                "{}: body differs at byte {at}, wrote {} bytes against {}",
                name_of(path),
                rewritten.len(),
                body.len()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} save bodies did not round trip:\n{}",
        failures.len(),
        saves.len(),
        failures.join("\n")
    );
    println!("{} save bodies round tripped byte for byte", saves.len());
}

/// The numbered corpus is in play order, so a stage marked done in one save
/// must still be done in every later one. That is a far stronger check than
/// any single file gives, because it tests the decode against the shape of a
/// real playthrough rather than against itself.
///
/// Radiant quests are the exception the game itself creates: `ResetQuest`
/// clears their stages so the quest can run again for a new target. Those
/// show up as a stage going from done back to not done, which is reported
/// rather than treated as a decode failure.
#[test]
fn quest_stages_only_advance_through_the_numbered_corpus() {
    let saves: Vec<_> = corpus()
        .into_iter()
        .filter(|path| {
            name_of(path)
                .chars()
                .take(2)
                .all(|character| character.is_ascii_digit())
        })
        .collect();
    if saves.len() < 2 {
        eprintln!("the numbered corpus is not present, skipping");
        return;
    }

    let mut previous: Option<(String, std::collections::BTreeMap<u32, Vec<i16>>)> = None;
    let mut resets = Vec::new();
    let mut advanced = 0usize;

    for path in &saves {
        let save = Save::open(path).expect("the corpus parses, see the parse test");
        let mut done: std::collections::BTreeMap<u32, Vec<i16>> = std::collections::BTreeMap::new();
        for (form, stages) in save.quest_stages().expect("stages decode") {
            let mut set: Vec<i16> = stages
                .iter()
                .filter(|stage| stage.is_done())
                .map(|stage| stage.stage)
                .collect();
            set.sort_unstable();
            if !set.is_empty() {
                done.insert(form, set);
            }
        }

        if let Some((earlier_name, earlier)) = &previous {
            for (form, stages) in earlier {
                let now = done.get(form);
                for stage in stages {
                    let still_done = now.is_some_and(|set| set.contains(stage));
                    if still_done {
                        advanced += 1;
                    } else {
                        resets.push(format!(
                            "{form:#010x} stage {stage} was done in {earlier_name} and is not in {}",
                            name_of(path)
                        ));
                    }
                }
            }
        }
        previous = Some((name_of(path), done));
    }

    println!(
        "{} saves in play order, {advanced} stage carry overs held, {} cleared",
        saves.len(),
        resets.len()
    );
    for reset in resets.iter().take(20) {
        println!("  cleared: {reset}");
    }

    // A handful of radiant quests reset legitimately. A decode error would
    // not be a handful, it would be most of them, so the bar is set against
    // the total rather than at zero.
    let total = advanced + resets.len();
    assert!(
        resets.len() * 20 < total,
        "{} of {total} stage carry overs were lost, which is too many to be radiant quests resetting",
        resets.len()
    );
}
