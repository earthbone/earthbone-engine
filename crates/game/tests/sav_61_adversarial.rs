// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for explicit recovery session boundaries.

use soe_esm::LoadOrder;
use soe_game::{save_generations::GenerationStore, Game};
use soe_papyrus::{asm::{i, ScriptBuilder}, Value};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "soe-sav-61-adversarial-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn store(&self) -> GenerationStore {
        let path = self.0.join("accepted");
        fs::create_dir(&path).unwrap();
        GenerationStore::new(path, 4).unwrap()
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn game(value: i32, script: &str) -> Game {
    let mut game = Game::new(LoadOrder::new());
    game.load_script(
        &ScriptBuilder::new(script, "")
            .variable("value", "Int", i(0))
            .bytes(),
    )
    .unwrap();
    let object = game.vm.create_object("recovery adversarial fixture");
    game.vm.attach(object, script).unwrap();
    game.vm
        .set_var(object, script, "value", Value::Int(value))
        .unwrap();
    game
}

#[test]
fn sav_61_newest_incompatible_generation_falls_back_to_compatible_state() {
    let directory = Directory::new();
    let store = directory.store();
    let old = game(12, "Sav61Compatible");
    let first = old.write_generation(&store).unwrap();
    game(99, "Sav61ChangedSchema")
        .write_generation(&store)
        .unwrap();

    let mut resumed = game(0, "Sav61Compatible");
    let selected = resumed.recover_generation(&store).unwrap();

    assert_eq!(selected.generation, first.generation);
    assert_eq!(resumed.save_state().unwrap(), old.save_state().unwrap());
    assert_eq!(selected.rejected.len(), 1);
    assert_eq!(selected.rejected[0].generation, first.generation + 1);
}

#[test]
fn sav_61_rejected_generation_does_not_replace_source_or_live_state() {
    let directory = Directory::new();
    let source = directory.0.join("source.ess");
    let original = b"original vanilla save bytes";
    fs::write(&source, original).unwrap();
    let store = directory.store();
    game(12, "Sav61OldSchema")
        .write_generation(&store)
        .unwrap();
    let mut live = game(99, "Sav61CurrentSchema");
    let before = live.save_state().unwrap();

    assert!(live.recover_generation(&store).is_err());

    assert_eq!(live.save_state().unwrap(), before);
    assert_eq!(fs::read(source).unwrap(), original);
    assert_eq!(fs::read_dir(directory.0.join("accepted")).unwrap().count(), 2);
}
