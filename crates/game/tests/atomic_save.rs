// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Synthetic native and ESS files, including failures in the public writers.

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_papyrus::Value;
use soe_papyrus::asm::{ScriptBuilder, i};
use soe_save::atomic::WriteError;
use soe_save::{Compression, Header, Save, WString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root =
            std::env::var_os("SOE_ATOMIC_TEST_ROOT").map_or_else(std::env::temp_dir, PathBuf::from);
        let path = root.join(format!(
            "soe-atomic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn destination(&self) -> PathBuf {
        self.0.join("save.dat")
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum Format {
    Native,
    Ess,
}

const FORMATS: [Format; 2] = [Format::Native, Format::Ess];

fn game(generation: i32) -> Game {
    let mut game = Game::new(LoadOrder::new());
    let script = ScriptBuilder::new("AtomicFixture", "")
        .variable("generation", "Int", i(0))
        .bytes();
    game.load_script(&script).unwrap();
    let object = game.vm.create_object(&"synthetic padding ".repeat(8192));
    game.vm.attach(object, "AtomicFixture").unwrap();
    game.vm
        .set_var(
            object,
            "AtomicFixture",
            "generation",
            Value::Int(generation),
        )
        .unwrap();
    game
}

fn ess(generation: u8) -> Save {
    let header = Header {
        version: 12,
        save_number: u32::from(generation),
        player_name: WString::from_text("Synthetic"),
        player_level: 1,
        player_location: WString::from_text("Fixture"),
        game_date: WString::from_text("1.1.1"),
        player_race: WString::from_text("NordRace"),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 1.0,
        filetime: 0,
        screenshot_width: 1,
        screenshot_height: 1,
        compression: Compression::None,
        screenshot: vec![0; 4],
    };
    let mut bytes = Vec::new();
    header.write(&mut bytes);
    let header_len = bytes.len();
    bytes.push(78);
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 3]);
    let table_at = bytes.len() + 100;
    let ids_at = table_at + 8;
    for value in [
        ids_at, ids_at, table_at, table_at, table_at, table_at, 0, 0, 0, 0,
    ] {
        bytes.extend_from_slice(&(value as u32).to_le_bytes());
    }
    bytes.extend_from_slice(&[0; 60]);
    assert_eq!(bytes.len(), table_at);
    // Table three's on-disk count excludes one entry. This opaque synthetic
    // block exercises the container contract without claiming runtime import.
    bytes.extend_from_slice(&1005u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&vec![generation; 160 * 1024]);
    let save = Save::parse(&bytes).unwrap();
    assert_eq!(save.header_len(), header_len);
    save
}

impl Format {
    fn write(self, path: &Path, generation: u8) -> Result<(), WriteError> {
        match self {
            Self::Native => game(i32::from(generation)).write_file(path),
            Self::Ess => ess(generation).write_file(path),
        }
    }

    fn bytes(self, generation: u8) -> Vec<u8> {
        match self {
            Self::Native => game(i32::from(generation)).save_state().unwrap(),
            Self::Ess => ess(generation).write().unwrap(),
        }
    }

    fn read(self, path: &Path) -> u8 {
        let bytes = fs::read(path).unwrap();
        let generation = match self {
            Self::Native => {
                let mut loaded = game(0);
                loaded.load_state(&bytes).unwrap();
                assert_eq!(loaded.save_state().unwrap(), bytes);
                if bytes == self.bytes(1) { 1 } else { 2 }
            }
            Self::Ess => {
                let loaded = Save::open(path).unwrap();
                assert_eq!(loaded.write().unwrap(), bytes);
                assert!(
                    loaded
                        .unknown_3
                        .iter()
                        .all(|byte| *byte == loaded.header.save_number as u8)
                );
                loaded.header.save_number as u8
            }
        };
        assert!(generation == 1 || generation == 2);
        assert_eq!(bytes, self.bytes(generation));
        generation
    }
}

#[test]
fn both_file_apis_replace_and_read_back() {
    for format in FORMATS {
        let directory = TestDir::new();
        let destination = directory.destination();
        format.write(&destination, 1).unwrap();
        assert_eq!(format.read(&destination), 1);
        format.write(&destination, 2).unwrap();
        assert_eq!(format.read(&destination), 2);
        assert_eq!(
            fs::read_dir(&directory.0).unwrap().count(),
            2,
            "only destination and persistent lock remain"
        );
    }
}

#[test]
fn compressed_ess_files_validate_before_replacement() {
    let directory = TestDir::new();
    for compression in [Compression::None, Compression::Zlib, Compression::Lz4] {
        let mut save = ess(2);
        save.header.compression = compression;
        save.write_file(&directory.destination()).unwrap();
        let loaded = Save::open(directory.destination()).unwrap();
        assert_eq!(loaded.header.compression, compression);
        assert_eq!(loaded.unknown_3, save.unknown_3);
    }
}

#[test]
fn invalid_native_world_vm_and_script_definitions_preserve_prior_and_live_state() {
    let directory = TestDir::new();
    let destination = directory.destination();
    let live = game(2);
    let bytes = live.save_state().unwrap();
    Format::Native.write(&destination, 1).unwrap();
    for magic in [b"SOEG", b"SOEW", b"SOEV"] {
        let mut invalid = bytes.clone();
        let offset = invalid.windows(4).position(|word| word == magic).unwrap();
        invalid[offset] = 0;
        assert!(matches!(
            live.write_state_file(&destination, &invalid),
            Err(WriteError::Validate(_))
        ));
        assert_eq!(Format::Native.read(&destination), 1);
        assert_eq!(live.save_state().unwrap(), bytes);
    }
    let no_scripts = Game::new(LoadOrder::new());
    assert!(matches!(
        no_scripts.write_state_file(&destination, &bytes),
        Err(WriteError::Validate(_))
    ));
    assert_eq!(Format::Native.read(&destination), 1);
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 2);
}

#[test]
fn invalid_ess_compressed_payload_preserves_prior() {
    let directory = TestDir::new();
    let destination = directory.destination();
    Format::Ess.write(&destination, 1).unwrap();
    let mut invalid = ess(2);
    invalid.change_forms.push(soe_save::change::ChangeForm {
        form: soe_save::RefId::from_bytes([0x40, 0, 1]),
        change_flags: 0,
        type_field: 0,
        version: 78,
        uncompressed_len: 64,
        data: vec![1, 2, 3],
    });
    assert!(matches!(
        invalid.write_file(&destination),
        Err(WriteError::Validate(_))
    ));
    assert_eq!(Format::Ess.read(&destination), 1);
}

#[cfg(unix)]
#[test]
fn links_and_directories_are_rejected_without_touching_their_contents() {
    use std::os::unix::fs::symlink;
    for format in FORMATS {
        let directory = TestDir::new();
        let real = directory.0.join("real.dat");
        format.write(&real, 1).unwrap();
        symlink(&real, directory.destination()).unwrap();
        assert!(format.write(&directory.destination(), 2).is_err());
        assert_eq!(format.read(&real), 1);
        fs::remove_file(directory.destination()).unwrap();
        fs::create_dir(directory.destination()).unwrap();
        assert!(format.write(&directory.destination(), 2).is_err());
        assert!(directory.destination().is_dir());
    }
}

#[cfg(feature = "save-fault-tests")]
mod failures {
    use super::*;
    use soe_save::atomic::{Step, fault};
    use std::io::{self, BufRead, Write};
    use std::process::{Child, Command, Stdio};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    // Acceptance boundaries are explicit. Adding an operation to the writer
    // must not silently shrink this failure matrix.
    const STEPS: [Step; 17] = [
        Step::Directory,
        Step::ProbeDirectorySync,
        Step::Lock,
        Step::InspectDestination,
        Step::CreateTemporary,
        Step::WriteTemporary,
        Step::SyncTemporary,
        Step::ValidateTemporary,
        Step::SyncTemporaryDirectory,
        Step::OpenPrevious,
        Step::SyncPrevious,
        Step::LinkPrevious,
        Step::SyncPreviousDirectory,
        Step::Replace,
        Step::SyncReplacementDirectory,
        Step::RemovePrevious,
        Step::SyncCleanupDirectory,
    ];

    fn prior_links(directory: &Path) -> Vec<PathBuf> {
        fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "prior"))
            .collect()
    }

    fn assert_recoverable(format: Format, directory: &TestDir) {
        format.read(&directory.destination());
        for path in prior_links(&directory.0) {
            assert_eq!(format.read(&path), 1);
        }
    }

    #[test]
    fn every_io_boundary_preserves_a_readable_generation() {
        let mut cases = 0;
        for format in FORMATS {
            for step in STEPS {
                for after in [false, true] {
                    let directory = TestDir::new();
                    let destination = directory.destination();
                    format.write(&destination, 1).unwrap();
                    let result = fault::with_hook(
                        move |actual, actual_after| {
                            if actual == step && actual_after == after {
                                Err(io::Error::other("injected persistence failure"))
                            } else {
                                Ok(())
                            }
                        },
                        || format.write(&destination, 2),
                    );
                    let Err(WriteError::Io {
                        step: actual,
                        staged,
                        previous,
                        replaced,
                        ..
                    }) = result
                    else {
                        panic!("missing injected failure for {format:?} {step:?} after={after}");
                    };
                    assert_eq!(actual, step);
                    assert_eq!(format.read(&destination), if replaced { 2 } else { 1 });
                    assert_recoverable(format, &directory);
                    if let Some(path) = previous {
                        assert_eq!(format.read(&path), 1);
                    }
                    if step == Step::WriteTemporary && after {
                        let partial = fs::read(staged.unwrap()).unwrap();
                        assert_eq!(partial.len(), 64 * 1024);
                        assert!(partial.len() < format.bytes(2).len());
                    }
                    // Retrying must leave retained evidence from the failure alone.
                    let evidence = prior_links(&directory.0);
                    format.write(&destination, 2).unwrap();
                    assert_eq!(format.read(&destination), 2);
                    for path in evidence {
                        assert_eq!(format.read(&path), 1);
                    }
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 68);
    }

    #[test]
    fn staged_corruption_is_detected_before_replacement() {
        for format in FORMATS {
            let directory = TestDir::new();
            let destination = directory.destination();
            format.write(&destination, 1).unwrap();
            let root = directory.0.clone();
            let result = fault::with_hook(
                move |step, after| {
                    if step == Step::ValidateTemporary && !after {
                        let stage = fs::read_dir(&root)?
                            .map(|entry| entry.unwrap().path())
                            .find(|path| {
                                path.file_name()
                                    .unwrap()
                                    .to_string_lossy()
                                    .contains(".soe-stage-")
                            })
                            .unwrap();
                        let mut bytes = fs::read(&stage)?;
                        bytes[0] ^= 0xff;
                        fs::write(stage, bytes)?;
                    }
                    Ok(())
                },
                || format.write(&destination, 2),
            );
            assert!(matches!(
                result,
                Err(WriteError::Io {
                    step: Step::ValidateTemporary,
                    replaced: false,
                    ..
                })
            ));
            assert_eq!(format.read(&destination), 1);
        }
    }

    fn paused_writer(format: Format, directory: &TestDir, step: Step, after: bool) -> Child {
        let test = crate::common::child_test_name(module_path!(), "child_writer");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test, "--nocapture"])
            .env("SOE_ATOMIC_CHILD_PATH", directory.destination())
            .env("SOE_ATOMIC_CHILD_FORMAT", format!("{format:?}"))
            .env("SOE_ATOMIC_CHILD_STEP", format!("{step:?}"))
            .env("SOE_ATOMIC_CHILD_AFTER", after.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, receiver) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in io::BufReader::new(stdout).lines() {
                if line.unwrap() == "AT_PERSISTENCE_BOUNDARY" {
                    sender.send(()).unwrap();
                    return;
                }
            }
        });
        let reached = receiver.recv_timeout(Duration::from_secs(10));
        if let Err(error) = reached {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            reader.join().unwrap();
            panic!(
                "writer failed at {format:?} {step:?} after={after}: {error}, stderr={}",
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
        let Some(path) = std::env::var_os("SOE_ATOMIC_CHILD_PATH") else {
            return;
        };
        let format = match std::env::var("SOE_ATOMIC_CHILD_FORMAT").unwrap().as_str() {
            "Native" => Format::Native,
            "Ess" => Format::Ess,
            other => panic!("invalid fixture format {other}"),
        };
        let step = std::env::var("SOE_ATOMIC_CHILD_STEP").unwrap();
        let after = std::env::var("SOE_ATOMIC_CHILD_AFTER").unwrap() == "true";
        fault::with_hook(
            move |actual, actual_after| {
                if format!("{actual:?}") == step && actual_after == after {
                    println!("AT_PERSISTENCE_BOUNDARY");
                    io::stdout().flush()?;
                    loop {
                        std::thread::park();
                    }
                }
                Ok(())
            },
            || format.write(Path::new(&path), 2),
        )
        .unwrap();
        panic!("writer never reached its requested boundary");
    }

    #[test]
    fn tst_06_process_termination_at_every_boundary_preserves_a_readable_generation() {
        let mut cases = 0;
        for format in FORMATS {
            for step in STEPS {
                for after in [false, true] {
                    let directory = TestDir::new();
                    format.write(&directory.destination(), 1).unwrap();
                    kill(paused_writer(format, &directory, step, after));
                    assert_recoverable(format, &directory);
                    let evidence = prior_links(&directory.0);
                    format.write(&directory.destination(), 2).unwrap();
                    assert_eq!(format.read(&directory.destination()), 2);
                    for path in evidence {
                        assert_eq!(format.read(&path), 1);
                    }
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 68);
    }

    #[test]
    fn first_save_termination_never_exposes_partial_bytes() {
        for format in FORMATS {
            for step in STEPS.into_iter().filter(|step| {
                !matches!(
                    step,
                    Step::OpenPrevious
                        | Step::SyncPrevious
                        | Step::LinkPrevious
                        | Step::SyncPreviousDirectory
                        | Step::RemovePrevious
                        | Step::SyncCleanupDirectory
                )
            }) {
                for after in [false, true] {
                    let directory = TestDir::new();
                    kill(paused_writer(format, &directory, step, after));
                    if directory.destination().exists() {
                        assert_eq!(format.read(&directory.destination()), 2);
                    }
                    format.write(&directory.destination(), 2).unwrap();
                    assert_eq!(format.read(&directory.destination()), 2);
                }
            }
        }
    }

    #[test]
    fn concurrent_writer_is_rejected_and_killed_writer_releases_lock() {
        for format in FORMATS {
            let directory = TestDir::new();
            format.write(&directory.destination(), 1).unwrap();
            let child = paused_writer(format, &directory, Step::Lock, true);
            let result = format.write(&directory.destination(), 2);
            assert!(matches!(
                result,
                Err(WriteError::Io {
                    step: Step::Lock,
                    ..
                })
            ));
            assert_eq!(format.read(&directory.destination()), 1);
            kill(child);
            format.write(&directory.destination(), 2).unwrap();
            assert_eq!(format.read(&directory.destination()), 2);
        }
    }

    #[test]
    fn persistence_order_covers_every_acceptance_boundary() {
        let directory = TestDir::new();
        Format::Native.write(&directory.destination(), 1).unwrap();
        let trace = Arc::new(Mutex::new(Vec::new()));
        let recorded = trace.clone();
        fault::with_hook(
            move |step, after| {
                if after {
                    recorded.lock().unwrap().push(step);
                }
                Ok(())
            },
            || Format::Native.write(&directory.destination(), 2),
        )
        .unwrap();
        let mut actual = trace.lock().unwrap().clone();
        actual.dedup();
        assert_eq!(actual, STEPS);
    }
}
