// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use soe_esm::LoadOrder;
use soe_game::Game;
use soe_save::atomic::{fault, Step};

fn fixture() -> Game {
    Game::new(LoadOrder::new())
}

#[test]
fn child_writer_pauses_after_replacement() {
    let Some(path) = std::env::var_os("SOE_TST06_PATH") else {
        return;
    };
    fault::with_hook(
        |step, after| {
            if step == Step::Replace && after {
                println!("TST06_REPLACED");
                use std::io::Write;
                std::io::stdout().flush()?;
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        },
        || fixture().write_file(PathBuf::from(path).as_path()),
    )
    .unwrap();
}

#[test]
fn tst_06_termination_after_replace_leaves_a_complete_loadable_save() {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "soe-tst06-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("save.dat");
    fixture().write_file(&path).unwrap();

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            &crate::common::child_test_name(
                module_path!(),
                "child_writer_pauses_after_replacement",
            ),
            "--nocapture",
        ])
        .env("SOE_TST06_PATH", &path)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        for line in BufReader::new(stdout).lines() {
            if line.unwrap() == "TST06_REPLACED" {
                sender.send(()).unwrap();
                return;
            }
        }
    });
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("writer reaches replacement boundary");
    reader.join().unwrap();
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());

    let bytes = fs::read(&path).unwrap();
    let mut loaded = fixture();
    loaded.load_state(&bytes).unwrap();
    assert_eq!(loaded.save_state().unwrap(), bytes);
    fs::remove_dir_all(directory).unwrap();
}
