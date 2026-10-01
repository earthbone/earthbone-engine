// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Owned synthetic native generations. No installed saves are read or changed.

use soe_esm::LoadOrder;
use soe_game::{
    save_generations::{Error, GenerationStore, RejectionReason},
    Game,
};
use soe_papyrus::asm::{i, ScriptBuilder};
use soe_papyrus::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::var_os("SOE_GENERATION_TEST_ROOT")
            .map_or_else(std::env::temp_dir, PathBuf::from);
        let path = root.join(format!(
            "soe-generations-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self, retained: usize) -> GenerationStore {
        GenerationStore::new(&self.0, retained).unwrap()
    }
    fn generation(&self, id: u64) -> PathBuf {
        self.0.join(format!("generation-{id:020}.soeb"))
    }
    fn ids(&self) -> Vec<u64> {
        let mut ids: Vec<_> = fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .filter_map(|name| {
                name.to_str().and_then(|name| {
                    name.strip_prefix("generation-")?
                        .strip_suffix(".soeb")?
                        .parse::<u64>()
                        .ok()
                })
            })
            .collect();
        ids.sort_unstable();
        ids
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn game_with_script(value: i32, script: &str) -> Game {
    let mut game = Game::new(LoadOrder::new());
    game.load_script(
        &ScriptBuilder::new(script, "")
            .variable("value", "Int", i(0))
            .bytes(),
    )
    .unwrap();
    let object = game.vm.create_object(&"synthetic padding ".repeat(8192));
    game.vm.attach(object, script).unwrap();
    game.vm
        .set_var(object, script, "value", Value::Int(value))
        .unwrap();
    game
}
fn game(value: i32) -> Game {
    game_with_script(value, "GenerationFixture")
}

fn snapshot(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut result: Vec<_> = fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let bytes = fs::read(&path).unwrap();
            (path, bytes)
        })
        .collect();
    result.sort();
    result
}

fn assert_loaded(directory: &Directory, expected: u64) {
    let mut fresh = game(0);
    let selected = fresh.recover_generation(&directory.store(2)).unwrap();
    assert_eq!(selected.generation, expected);
    assert_eq!(
        fresh.save_state().unwrap(),
        game(expected as i32).save_state().unwrap()
    );
    let mut independent = game(0);
    independent.load_state(selected.bytes()).unwrap();
    assert_eq!(
        independent.save_state().unwrap(),
        fresh.save_state().unwrap()
    );
}

#[test]
fn numbered_generations_have_deterministic_bounded_retention() {
    let directory = Directory::new();
    for id in 1..=6 {
        let result = game(id).write_generation(&directory.store(3)).unwrap();
        assert_eq!(result.generation, id as u64);
    }
    assert_eq!(directory.ids(), [4, 5, 6]);
    assert_eq!(
        fs::read_dir(&directory.0).unwrap().count(),
        4,
        "three generations and one store lock"
    );
    assert_loaded(&directory, 6);
    assert!(matches!(
        GenerationStore::new(&directory.0, 1),
        Err(Error::Retention(1))
    ));
    assert!(matches!(
        GenerationStore::new(&directory.0, 65),
        Err(Error::Retention(65))
    ));
}

#[test]
fn sav_63_corrupt_latest_recovers_an_intact_generation() {
    let directory = Directory::new();
    game(1).write_generation(&directory.store(2)).unwrap();
    game(2).write_generation(&directory.store(2)).unwrap();
    let mut bytes = fs::read(directory.generation(2)).unwrap();
    let changed = game(88).save_state().unwrap();
    let mut reader = soe_papyrus::vm::state::Reader::new(&bytes);
    assert_eq!(reader.take(4).unwrap(), b"SOEB");
    assert_eq!(reader.u16().unwrap(), 1);
    assert_eq!(reader.u64().unwrap(), 2);
    let payload_len = usize::try_from(reader.u64().unwrap()).unwrap();
    reader.u32().unwrap();
    assert_eq!(payload_len, reader.remaining());
    let payload_at = bytes.len() - reader.remaining();
    assert_eq!(payload_len, changed.len());
    game(0).load_state(&changed).unwrap();
    bytes[payload_at..].copy_from_slice(&changed);
    fs::write(directory.generation(2), bytes).unwrap();
    let before = snapshot(&directory.0);
    let mut live = game(77);
    let selected = live.select_generation(&directory.store(2)).unwrap();
    assert_eq!(selected.generation, 1);
    assert!(
        matches!(&selected.rejected[0].reason, RejectionReason::Integrity(reason) if reason.contains("checksum"))
    );
    assert_eq!(live.save_state().unwrap(), game(77).save_state().unwrap());
    live.recover_generation(&directory.store(2)).unwrap();
    assert_eq!(live.save_state().unwrap(), game(1).save_state().unwrap());
    assert_eq!(
        snapshot(&directory.0),
        before,
        "recovery leaves damaged evidence untouched"
    );
}

#[test]
fn corrupt_all_candidates_leaves_live_state_and_files_unchanged() {
    let directory = Directory::new();
    for id in 1..=2 {
        game(id).write_generation(&directory.store(2)).unwrap();
        fs::write(directory.generation(id as u64), b"damaged").unwrap();
    }
    // The successful second write prunes the already damaged first candidate.
    fs::write(directory.generation(1), b"also damaged").unwrap();
    let files = snapshot(&directory.0);
    let mut live = game(77);
    let before = live.save_state().unwrap();
    let Err(Error::NoLoadable { rejected }) = live.recover_generation(&directory.store(2)) else {
        panic!("corrupt generations accepted");
    };
    assert_eq!(
        rejected
            .iter()
            .map(|candidate| candidate.generation)
            .collect::<Vec<_>>(),
        [2, 1]
    );
    assert_eq!(live.save_state().unwrap(), before);
    assert_eq!(snapshot(&directory.0), files);
}

#[test]
fn incompatible_newer_generations_cannot_displace_loadable_fallbacks() {
    let directory = Directory::new();
    game(1).write_generation(&directory.store(3)).unwrap();
    game_with_script(2, "UnavailableScript")
        .write_generation(&directory.store(3))
        .unwrap();
    let selected = game(0).select_generation(&directory.store(2)).unwrap();
    assert_eq!(selected.generation, 1);
    assert!(matches!(
        selected.rejected[0].reason,
        RejectionReason::Definitions(_)
    ));
    let report = game(3).write_generation(&directory.store(2)).unwrap();
    assert_eq!(report.retained, [3, 1]);
    assert_eq!(directory.ids(), [1, 3]);
    fs::write(directory.generation(3), b"damaged").unwrap();
    assert_loaded(&directory, 1);
}

#[test]
fn empty_or_incompatible_store_does_not_mutate_the_game() {
    let directory = Directory::new();
    let mut live = game(77);
    let before = live.save_state().unwrap();
    assert!(matches!(
        live.recover_generation(&directory.store(2)),
        Err(Error::NoLoadable { .. })
    ));
    game_with_script(1, "AbsentScript")
        .write_generation(&directory.store(2))
        .unwrap();
    let Err(Error::NoLoadable { rejected }) = live.recover_generation(&directory.store(2)) else {
        panic!("missing scripts accepted");
    };
    assert!(matches!(
        rejected[0].reason,
        RejectionReason::Definitions(_)
    ));
    assert_eq!(live.save_state().unwrap(), before);
}

#[test]
fn rotation_only_removes_exact_owned_artifacts() {
    let directory = Directory::new();
    let untouched = [
        "notes.txt",
        ".other-save.soe-lock",
        ".generation-1.soeb.soe-lock",
        ".generation-00000000000000000001.soeb.soe-stage-not-a-pid-2",
        "generation-00000000000000000000.soeb",
    ];
    for name in untouched {
        fs::write(directory.0.join(name), b"user content").unwrap();
    }
    let artifact = directory
        .0
        .join(".generation-00000000000000000001.soeb.soe-stage-42-0");
    fs::write(&artifact, b"incomplete owned transaction").unwrap();
    game(1).write_generation(&directory.store(2)).unwrap();
    assert!(!artifact.exists());
    for name in untouched {
        assert_eq!(fs::read(directory.0.join(name)).unwrap(), b"user content");
    }
}

#[cfg(unix)]
#[test]
fn symlink_candidates_are_rejected_without_following_or_deleting_them() {
    let directory = Directory::new();
    game(1).write_generation(&directory.store(2)).unwrap();
    let user_file = directory.0.join("user-data");
    fs::write(&user_file, b"do not touch").unwrap();
    std::os::unix::fs::symlink(&user_file, directory.generation(2)).unwrap();
    let selected = game(0).select_generation(&directory.store(2)).unwrap();
    assert_eq!(selected.generation, 1);
    assert!(matches!(
        selected.rejected[0].reason,
        RejectionReason::Read(_)
    ));
    assert!(game(3).write_generation(&directory.store(2)).is_err());
    assert_eq!(fs::read(&user_file).unwrap(), b"do not touch");
    assert!(fs::symlink_metadata(directory.generation(2))
        .unwrap()
        .is_symlink());
    assert_loaded(&directory, 3);
}

#[test]
fn exhausted_generation_counter_preserves_existing_files() {
    let directory = Directory::new();
    game(1).write_generation(&directory.store(2)).unwrap();
    fs::write(directory.generation(u64::MAX), b"counter reservation").unwrap();
    let before = snapshot(&directory.0);
    assert!(matches!(
        game(2).write_generation(&directory.store(2)),
        Err(Error::CounterExhausted)
    ));
    assert_eq!(snapshot(&directory.0), before);
    assert_loaded(&directory, 1);
}

#[cfg(feature = "save-fault-tests")]
mod failures {
    use super::*;
    use soe_game::save_generations::{fault, Step};
    use soe_save::atomic::{fault as atomic_fault, Step as AtomicStep};
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::io::{self, BufRead, Write};
    use std::process::{Child, Command, Stdio};
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::time::Duration;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Event {
        Store(Step),
        Atomic(AtomicStep),
    }

    fn with_hooks<T>(
        hook: impl FnMut(Event, bool) -> io::Result<()> + 'static,
        action: impl FnOnce() -> T,
    ) -> T {
        let shared = Rc::new(RefCell::new(hook));
        let other = shared.clone();
        fault::with_hook(
            move |step, after| (shared.borrow_mut())(Event::Store(step), after),
            || {
                atomic_fault::with_hook(
                    move |step, after| (other.borrow_mut())(Event::Atomic(step), after),
                    action,
                )
            },
        )
    }

    fn boundaries() -> Vec<(Event, usize)> {
        // Four existing generations, retaining two, require three removals.
        // Two stale staging files and this write's lock require three cleanups.
        let mut result: Vec<_> = [
            (Step::OpenDirectory, 1),
            (Step::SyncInitialDirectory, 1),
            (Step::Lock, 1),
            (Step::Scan, 2),
            (Step::WriteGeneration, 1),
            (Step::VerifyGeneration, 1),
            (Step::SyncGeneration, 1),
            (Step::SyncPublishedDirectory, 1),
            (Step::InspectRetention, 1),
            (Step::RemoveGeneration, 3),
            (Step::SyncRemovalDirectory, 3),
            (Step::RemoveArtifact, 3),
            (Step::SyncArtifactDirectory, 3),
        ]
        .into_iter()
        .map(|(step, count)| (Event::Store(step), count))
        .collect();
        result.extend(
            [
                (AtomicStep::Directory, 1),
                (AtomicStep::ProbeDirectorySync, 1),
                (AtomicStep::Lock, 1),
                (AtomicStep::InspectDestination, 1),
                (AtomicStep::CreateTemporary, 1),
                (AtomicStep::WriteTemporary, 3),
                (AtomicStep::SyncTemporary, 1),
                (AtomicStep::ValidateTemporary, 1),
                (AtomicStep::SyncTemporaryDirectory, 1),
                (AtomicStep::Replace, 1),
                (AtomicStep::SyncReplacementDirectory, 1),
            ]
            .into_iter()
            .map(|(step, count)| (Event::Atomic(step), count)),
        );
        assert_eq!(result.iter().map(|(_, count)| count).sum::<usize>(), 35);
        result
    }

    fn seed() -> Directory {
        let directory = Directory::new();
        for id in 1..=4 {
            game(id).write_generation(&directory.store(4)).unwrap();
        }
        for serial in 0..2 {
            fs::write(
                directory.0.join(format!(
                    ".generation-00000000000000000005.soeb.soe-stage-42-{serial}"
                )),
                b"partial stage",
            )
            .unwrap();
        }
        assert_eq!(directory.ids(), [1, 2, 3, 4]);
        let size = game(5).save_state().unwrap().len() + 26;
        assert!(
            (128 * 1024 + 1..=192 * 1024).contains(&size),
            "fixture must require three atomic write chunks"
        );
        directory
    }

    fn verify_restart_and_retry(directory: &Directory) {
        let ids = directory.ids();
        assert!(
            ids.len() >= 2,
            "rotation must leave at least two intact generations"
        );
        for id in &ids {
            let bytes = fs::read(directory.generation(*id)).unwrap();
            let mut independent = game(0);
            independent.load_state(&bytes[26..]).unwrap();
            assert_eq!(
                independent.save_state().unwrap(),
                game(*id as i32).save_state().unwrap()
            );
        }
        let latest = *ids.last().unwrap();
        assert_loaded(directory, latest);
        let next = latest + 1;
        game(next as i32)
            .write_generation(&directory.store(2))
            .unwrap();
        assert_loaded(directory, next);
        assert_eq!(directory.ids().len(), 2);
        assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 3);
    }

    #[test]
    fn sav_63_every_write_boundary_preserves_recoverable_generations() {
        let mut cases = 0;
        for (event, count) in boundaries() {
            for occurrence in 1..=count {
                for after in [false, true] {
                    let directory = seed();
                    let mut seen = 0;
                    let result = with_hooks(
                        move |actual, actual_after| {
                            if actual == event && actual_after == after {
                                seen += 1;
                                if seen == occurrence {
                                    return Err(io::Error::other("injected generation failure"));
                                }
                            }
                            Ok(())
                        },
                        || game(5).write_generation(&directory.store(2)),
                    );
                    assert!(
                        result.is_err(),
                        "unvisited {event:?} occurrence={occurrence} after={after}"
                    );
                    verify_restart_and_retry(&directory);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 70);
    }

    fn spawn_paused(directory: &Directory, event: Event, occurrence: usize, after: bool) -> Child {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &crate::common::child_test_name(module_path!(), "child_writer"),
                "--nocapture",
            ])
            .env("SOE_GENERATION_CHILD_PATH", &directory.0)
            .env("SOE_GENERATION_CHILD_EVENT", format!("{event:?}"))
            .env("SOE_GENERATION_CHILD_OCCURRENCE", occurrence.to_string())
            .env("SOE_GENERATION_CHILD_AFTER", after.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in io::BufReader::new(stdout).lines() {
                if line.unwrap() == "AT_GENERATION_BOUNDARY" {
                    sender.send(()).unwrap();
                    return;
                }
            }
        });
        if let Err(error) = receiver.recv_timeout(Duration::from_secs(10)) {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            reader.join().unwrap();
            panic!(
                "writer missed {event:?} occurrence={occurrence} after={after}: {error}, stderr={}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        reader.join().unwrap();
        child
    }

    fn kill(mut child: Child) {
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
    }

    #[test]
    fn child_writer() {
        let Some(path) = std::env::var_os("SOE_GENERATION_CHILD_PATH") else {
            return;
        };
        let wanted = std::env::var("SOE_GENERATION_CHILD_EVENT").unwrap();
        let occurrence = std::env::var("SOE_GENERATION_CHILD_OCCURRENCE")
            .unwrap()
            .parse::<usize>()
            .unwrap();
        let after = std::env::var("SOE_GENERATION_CHILD_AFTER").unwrap() == "true";
        let mut seen = 0;
        with_hooks(
            move |actual, actual_after| {
                if format!("{actual:?}") == wanted && actual_after == after {
                    seen += 1;
                    if seen == occurrence {
                        println!("AT_GENERATION_BOUNDARY");
                        io::stdout().flush()?;
                        loop {
                            std::thread::park();
                        }
                    }
                }
                Ok(())
            },
            || game(5).write_generation(&GenerationStore::new(path, 2).unwrap()),
        )
        .unwrap();
        panic!("writer did not reach its boundary");
    }

    #[test]
    fn sav_63_termination_during_rotation_preserves_verified_generations() {
        let mut cases = 0;
        for (event, count) in boundaries() {
            for occurrence in 1..=count {
                for after in [false, true] {
                    let directory = seed();
                    kill(spawn_paused(&directory, event, occurrence, after));
                    verify_restart_and_retry(&directory);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 70);
    }

    #[test]
    fn explicit_boundary_matrix_covers_the_complete_write_trace() {
        let directory = seed();
        let actual = Rc::new(RefCell::new(BTreeMap::new()));
        let recorded = actual.clone();
        with_hooks(
            move |event, after| {
                if after {
                    *recorded
                        .borrow_mut()
                        .entry(format!("{event:?}"))
                        .or_insert(0) += 1;
                }
                Ok(())
            },
            || game(5).write_generation(&directory.store(2)),
        )
        .unwrap();
        let expected: BTreeMap<_, _> = boundaries()
            .into_iter()
            .map(|(event, count)| (format!("{event:?}"), count))
            .collect();
        assert_eq!(*actual.borrow(), expected);
    }

    #[test]
    fn damaged_new_generation_prevents_all_pruning() {
        let directory = seed();
        let path = directory.generation(5);
        let original: Vec<_> = (1..=4)
            .map(|id| fs::read(directory.generation(id)).unwrap())
            .collect();
        let result = with_hooks(
            move |event, after| {
                if event == Event::Store(Step::VerifyGeneration) && !after {
                    fs::write(&path, b"damaged after publication")?;
                }
                Ok(())
            },
            || game(5).write_generation(&directory.store(2)),
        );
        assert!(matches!(result, Err(Error::Verification { .. })));
        assert_loaded(&directory, 4);
        for (index, bytes) in original.iter().enumerate() {
            assert_eq!(
                &fs::read(directory.generation(index as u64 + 1)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn sav_63_corrupt_latest_after_interrupted_rotation_keeps_last_verified() {
        for occurrence in 1..=3 {
            for after in [false, true] {
                let directory = seed();
                let prior = fs::read(directory.generation(4)).unwrap();
                kill(spawn_paused(
                    &directory,
                    Event::Store(Step::RemoveGeneration),
                    occurrence,
                    after,
                ));
                fs::write(directory.generation(5), b"damaged after publication").unwrap();

                let mut recovered = game(77);
                let selected = recovered.recover_generation(&directory.store(2)).unwrap();
                assert_eq!(selected.generation, 4);
                assert!(matches!(
                    selected.rejected[0].reason,
                    RejectionReason::Integrity(_)
                ));
                assert_eq!(
                    recovered.save_state().unwrap(),
                    game(4).save_state().unwrap()
                );
                assert_eq!(fs::read(directory.generation(4)).unwrap(), prior);

                let report = game(6).write_generation(&directory.store(2)).unwrap();
                assert_eq!(report.retained, [6, 4]);
                assert_eq!(directory.ids(), [4, 6]);
            }
        }
    }

    #[test]
    fn read_failures_never_mutate_live_state() {
        let directory = seed();
        for after in [false, true] {
            let mut live = game(77);
            let before = live.save_state().unwrap();
            let result = with_hooks(
                move |event, actual_after| {
                    if event == Event::Store(Step::ReadRecovery) && actual_after == after {
                        Err(io::Error::other("injected read failure"))
                    } else {
                        Ok(())
                    }
                },
                || live.recover_generation(&directory.store(2)),
            );
            assert!(result.is_err());
            assert_eq!(live.save_state().unwrap(), before);
        }
    }

    #[test]
    fn store_lock_serializes_rotation_and_selection_and_releases_on_kill() {
        let directory = seed();
        let child = spawn_paused(&directory, Event::Store(Step::Lock), 1, true);
        assert!(matches!(
            game(5).write_generation(&directory.store(2)),
            Err(Error::Io {
                step: Step::Lock,
                ..
            })
        ));
        assert!(matches!(
            game(0).select_generation(&directory.store(2)),
            Err(Error::Io {
                step: Step::Lock,
                ..
            })
        ));
        kill(child);
        verify_restart_and_retry(&directory);
    }
}
