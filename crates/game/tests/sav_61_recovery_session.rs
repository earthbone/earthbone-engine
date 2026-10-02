// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Available recovery-session boundary while dependency recovery is unfinished.

use soe_esm::LoadOrder;
use soe_game::{Game, save_generations::GenerationStore};
use soe_papyrus::{
    Value,
    asm::{ScriptBuilder, i},
};
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
            "soe-sav-61-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn recovery_store(&self) -> GenerationStore {
        let path = self.0.join("accepted-recovery");
        fs::create_dir(&path).unwrap();
        GenerationStore::new(path, 2).unwrap()
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
    let object = game.vm.create_object("recovery session fixture");
    game.vm.attach(object, script).unwrap();
    game.vm
        .set_var(object, script, "value", Value::Int(value))
        .unwrap();
    game
}

#[test]
fn sav_61_recovery_output_is_distinct_and_keeps_source_bytes() {
    let directory = Directory::new();
    let source = directory.0.join("source.ess");
    let source_bytes = b"preserved source save fixture";
    fs::write(&source, source_bytes).unwrap();

    let store = directory.recovery_store();
    game(12, "Sav61Compatible")
        .write_generation(&store)
        .unwrap();
    let mut resumed = game(99, "Sav61Compatible");
    let selected = resumed.recover_generation(&store).unwrap();

    assert_ne!(selected.path, source);
    assert!(
        selected
            .path
            .starts_with(directory.0.join("accepted-recovery"))
    );
    assert_eq!(fs::read(source).unwrap(), source_bytes);
    assert_eq!(
        resumed.save_state().unwrap(),
        game(12, "Sav61Compatible").save_state().unwrap()
    );
}

#[test]
fn sav_61_incompatible_recovery_is_refused_without_live_state_changes() {
    let directory = Directory::new();
    let source = directory.0.join("source.ess");
    let source_bytes = b"preserved source save fixture";
    fs::write(&source, source_bytes).unwrap();

    let store = directory.recovery_store();
    game(12, "Sav61OldSchema").write_generation(&store).unwrap();
    let mut live = game(99, "Sav61CurrentSchema");
    let before = live.save_state().unwrap();

    let error = live.recover_generation(&store).unwrap_err();

    assert!(error.to_string().contains("no generation"));
    assert_eq!(live.save_state().unwrap(), before);
    assert_eq!(fs::read(source).unwrap(), source_bytes);
}
