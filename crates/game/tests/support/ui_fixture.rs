// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared plugin and script fixture setup for game and host MCM checks.

use soe_esm::{LoadOrder, Plugin};
use soe_game::Game;
use std::path::Path;

pub struct Fixture {
    pub plugin: Plugin,
    pub scripts: Vec<(String, Vec<u8>)>,
}

impl Fixture {
    pub fn game(&self) -> Game {
        let mut order = LoadOrder::new();
        order.add("Ui05Config.esm", self.plugin.clone()).unwrap();
        let mut game = Game::new(order);
        for (_, bytes) in &self.scripts {
            game.load_script(bytes).unwrap();
        }
        let bound = game.start();
        assert!(bound.scripts_missing.is_empty(), "{bound:?}");
        game
    }

    /// Write a standalone plugin and loose scripts for the host checks.
    #[allow(dead_code)]
    pub fn write(&self, directory: &Path) {
        std::fs::create_dir_all(directory.join("Scripts")).unwrap();
        std::fs::write(directory.join("Ui05Config.esm"), self.plugin.to_bytes()).unwrap();
        for (name, bytes) in &self.scripts {
            std::fs::write(directory.join("Scripts").join(format!("{name}.pex")), bytes).unwrap();
        }
    }
}
