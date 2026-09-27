// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::{save_generations::GenerationStore, Game};
use soe_papyrus::{asm::{i, ScriptBuilder}, Value};
use std::{fs, path::PathBuf, sync::atomic::{AtomicU64, Ordering}};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir();
        let path = root.join(format!(
            "soe-sav-63-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn generation(&self, id: u64) -> PathBuf {
        self.0.join(format!("generation-{id:020}.soeb"))
    }

    fn ids(&self) -> Vec<u64> {
        let mut ids: Vec<_> = fs::read_dir(&self.0)
            .unwrap()
            .filter_map(|entry| {
                entry
                    .ok()?
                    .file_name()
                    .to_str()?
                    .strip_prefix("generation-")?
                    .strip_suffix(".soeb")?
                    .parse()
                    .ok()
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

fn game(value: i32) -> Game {
    let mut game = Game::new(LoadOrder::new());
    game.load_script(
        &ScriptBuilder::new("Sav63Fixture", "")
            .variable("value", "Int", i(0))
            .bytes(),
    )
    .unwrap();
    let object = game.vm.create_object("generation payload");
    game.vm.attach(object, "Sav63Fixture").unwrap();
    game.vm
        .set_var(object, "Sav63Fixture", "value", Value::Int(value))
        .unwrap();
    game
}

#[test]
fn damaged_latest_after_partial_rotation_recovers_and_retry_keeps_fallback() {
    let directory = Directory::new();
    for value in 1..=5 {
        game(value)
            .write_generation(&GenerationStore::new(&directory.0, 4).unwrap())
            .unwrap();
    }
    // Model a writer stopped after pruning two old generations.
    fs::remove_file(directory.generation(2)).unwrap();
    fs::remove_file(directory.generation(3)).unwrap();
    fs::write(directory.generation(5), b"damaged after publication").unwrap();
    let verified_fallback = fs::read(directory.generation(4)).unwrap();

    let mut recovered = game(77);
    let selected = recovered
        .recover_generation(&GenerationStore::new(&directory.0, 2).unwrap())
        .unwrap();
    assert_eq!(selected.generation, 4);
    assert_eq!(
        recovered.save_state().unwrap(),
        game(4).save_state().unwrap()
    );
    assert_eq!(fs::read(directory.generation(4)).unwrap(), verified_fallback);

    let report = game(6)
        .write_generation(&GenerationStore::new(&directory.0, 2).unwrap())
        .unwrap();
    assert_eq!(report.retained, [6, 4]);
    assert_eq!(directory.ids(), [4, 6]);
    let mut retry = game(0);
    assert_eq!(
        retry
            .recover_generation(&GenerationStore::new(&directory.0, 2).unwrap())
            .unwrap()
            .generation,
        6
    );
}
