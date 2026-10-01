// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-03 component-mod acceptance check. A Rust `wasm32-wasip2` guest
//! subscribes to `OnActivate` through the versioned WIT API, then advances
//! the synthetic quest through the host's shared quest state.

use std::path::{Path, PathBuf};
use std::process::Command;

use soe_esm::{Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};

const TAG: fn(&[u8; 4]) -> Tag = Tag::new;
const QUEST: u32 = 0x800;
const ACTIVATOR_BASE: u32 = 0x801;
const CELL: u32 = 0x802;
const ACTIVATOR: u32 = 0x803;

fn plugin() -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    let quest = quest::with_stage(
        quest::build_record(QUEST, "Scr03Quest", 0),
        10,
        0,
        0,
        "Component activated",
    );
    plugin.push_top(quest);
    plugin.push_top(Record::new(TAG(b"ACTI"), ACTIVATOR_BASE));

    let cell_index =
        plugin.push_record(Record::new(TAG(b"CELL"), CELL).sub(TAG(b"DATA"), [1u8, 0]));
    let activator_index = plugin.push_record(
        Record::new(TAG(b"REFR"), ACTIVATOR)
            .sub(TAG(b"NAME"), ACTIVATOR_BASE.to_le_bytes())
            .sub(TAG(b"DATA"), vec![0; 24]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(activator_index));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut subblock = Group::new(GroupKind::InteriorSubBlock(0));
    subblock.entries.push(Entry::Record(cell_index));
    subblock.entries.push(Entry::Group(children));
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(subblock));
    let mut top = Group::new(GroupKind::Top(TAG(b"CELL")));
    top.entries.push(Entry::Group(block));
    plugin.groups.push(top);
    plugin
}

fn fixture() -> Game {
    let mut order = LoadOrder::new();
    order.add("Scr03.esm", plugin()).unwrap();
    Game::new(order)
}

fn wasm_component() -> PathBuf {
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scr_03_mod/Cargo.toml");
    let active = Command::new("rustup")
        .args(["show", "active-toolchain"])
        .output()
        .expect("rustup must report the active toolchain");
    assert!(active.status.success(), "rustup must select a toolchain");
    let toolchain = String::from_utf8(active.stdout).unwrap();
    let toolchain = toolchain.split_whitespace().next().unwrap();
    let compiler = Command::new("rustup")
        .args(["which", "rustc", "--toolchain", toolchain])
        .output()
        .expect("rustup must locate the active compiler");
    assert!(compiler.status.success(), "rustup must locate rustc");
    let compiler = String::from_utf8(compiler.stdout).unwrap();
    let compiler = compiler.trim();
    let mut cargo = Command::new("rustup");
    cargo
        .args(["run", toolchain, "cargo", "build"])
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target=wasm32-wasip2");
    cargo.env("RUSTC", compiler);
    let status = cargo
        .status()
        .expect("cargo must build the guest component");
    assert!(status.success(), "Rust wasm mod fixture did not compile");

    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.parent().unwrap().join("target"));
    target_dir.join("wasm32-wasip2/debug/scr_03_mod.wasm")
}

#[test]
fn scr_03_rust_wasm_component_subscribes_and_advances_quest_stage() {
    let component = std::fs::read(wasm_component()).unwrap();
    let mut game = fixture();
    game.load_wasm_mod(&component).unwrap();

    assert!(game.activate(
        soe_esm::GlobalFormId {
            file: 0,
            object: ACTIVATOR,
        },
        soe_esm::GlobalFormId {
            file: 0,
            object: ACTIVATOR,
        },
    ));
    assert_eq!(
        game.world_mut()
            .quest_state(soe_esm::GlobalFormId {
                file: 0,
                object: QUEST,
            })
            .unwrap()
            .current,
        10,
        "the OnActivate component should call SetStage on the synthetic quest"
    );
}
