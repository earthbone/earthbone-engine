// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! External, unmodified SkyUI PEX inputs with a minimal config quest.
//! The callbacks follow local SkyUI's ConfigMenuTemplate.psc interface.

use soe_esm::{plugin::flags, FileHeader, LoadOrder, Plugin, Tag};
use soe_game::quest::{self, flags as quest_flags};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;
use std::path::{Path, PathBuf};

pub const CONFIG: u32 = 0x800;
pub const MANAGER: u32 = 0x801;
pub const CHILD: &str = "Ui05ExecutableChild";

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

    pub fn write(&self, directory: &Path) {
        std::fs::create_dir_all(directory.join("Scripts")).unwrap();
        std::fs::write(directory.join("Ui05Config.esm"), self.plugin.to_bytes()).unwrap();
        for (name, bytes) in &self.scripts {
            std::fs::write(directory.join("Scripts").join(format!("{name}.pex")), bytes).unwrap();
        }
    }
}

pub fn load() -> Option<Fixture> {
    let mut vfs = soe_bsa::Vfs::new();
    if let Some(path) = std::env::var_os("SOE_TEST_SKYUI") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            vfs.add_loose_root(&path).unwrap();
            let mut archives: Vec<_> = std::fs::read_dir(&path)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("bsa"))
                })
                .collect();
            archives.sort();
            for archive in archives {
                vfs.add_archive_path(&archive).unwrap();
            }
        } else {
            vfs.add_archive_path(&path).unwrap();
        }
    } else if let Some(path) = std::env::var_os("SOE_SKYUI_CONFIG_BASE_PEX") {
        let path = PathBuf::from(path);
        let mut scripts = Vec::new();
        for name in ["SKI_ConfigBase", "SKI_ConfigManager", "SKI_QuestBase"] {
            let input = if name == "SKI_ConfigBase" {
                path.clone()
            } else {
                path.with_file_name(format!("{name}.pex"))
            };
            scripts.push((name.to_owned(), std::fs::read(input).unwrap()));
        }
        return Some(build(scripts));
    } else {
        eprintln!("UI-05 skipped: set SOE_TEST_SKYUI to an extracted SkyUI root or BSA");
        return None;
    }
    let scripts = ["SKI_ConfigBase", "SKI_ConfigManager", "SKI_QuestBase"]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                vfs.read(&format!("scripts/{name}.pex")).unwrap(),
            )
        })
        .collect();
    Some(build(scripts))
}

fn build(mut scripts: Vec<(String, Vec<u8>)>) -> Fixture {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    for (id, name) in [(CONFIG, CHILD), (MANAGER, "SKI_ConfigManager")] {
        let vmad = Vmad {
            scripts: vec![ScriptEntry::new(name)],
            ..Default::default()
        };
        plugin.push_top(
            quest::build_record(id, name, quest_flags::START_GAME_ENABLED)
                .sub(Tag::new(b"VMAD"), vmad.write(Tag::new(b"QUST"))),
        );
    }
    let mut child = ScriptBuilder::new(CHILD, "SKI_ConfigBase")
        .variable("init_count", "Int", i(0))
        .variable("register_count", "Int", i(0))
        .variable("reset_count", "Int", i(0))
        .variable("open_count", "Int", i(0))
        .variable("close_count", "Int", i(0))
        .variable("sequence", "Int", i(0))
        .variable("last_page", "String", s("unset"))
        .function(
            FunctionBuilder::new("OnConfigInit", "None")
                .local("pages", "String[]")
                .ins(IAdd, vec![ident("init_count"), ident("init_count"), i(1)])
                .ins(PropSet, vec![s("ModName"), ident("self"), s("Example Mod")])
                .ins(ArrayCreate, vec![ident("pages"), i(2)])
                .ins(ArraySetElement, vec![ident("pages"), i(0), s("General")])
                .ins(ArraySetElement, vec![ident("pages"), i(1), s("Advanced")])
                .ins(PropSet, vec![s("Pages"), ident("self"), ident("pages")])
                .build(),
        );
    for (event, counter, digit) in [
        ("OnConfigRegister", "register_count", 0),
        ("OnPageReset", "reset_count", 1),
        ("OnConfigOpen", "open_count", 2),
        ("OnConfigClose", "close_count", 3),
    ] {
        let mut callback = FunctionBuilder::new(event, "None")
            .ins(IAdd, vec![ident(counter), ident(counter), i(1)]);
        if event == "OnPageReset" {
            callback = callback
                .param("page", "String")
                .ins(Assign, vec![ident("last_page"), ident("page")]);
        }
        if digit != 0 {
            callback = callback
                .ins(IMul, vec![ident("sequence"), ident("sequence"), i(10)])
                .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(digit)]);
        }
        child = child.function(callback.build());
    }
    scripts.push((CHILD.to_owned(), child.bytes()));
    Fixture { plugin, scripts }
}
