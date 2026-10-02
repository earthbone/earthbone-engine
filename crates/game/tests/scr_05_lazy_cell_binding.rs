// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SCR-05: placed-reference scripts bind when their parent cell loads.

use soe_esm::{
    Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag,
    plugin::flags,
};
use soe_game::vmad::{ScriptEntry, Vmad};
use soe_game::{BindReport, Game};
use soe_papyrus::asm::ScriptBuilder;
use std::path::PathBuf;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const CELL: u32 = 0x800;
pub const REFERENCE: u32 = 0x801;

pub fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    let cell_index = plugin.push_record(
        Record::new(T(b"CELL"), CELL)
            .zsub(T(b"EDID"), "Scr05Cell")
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let vmad = Vmad {
        scripts: vec![
            ScriptEntry::new("Scr05Present"),
            ScriptEntry::new("Scr05Absent"),
        ],
        ..Default::default()
    }
    .write(T(b"REFR"));
    let placed = plugin.push_record(
        Record::new(T(b"REFR"), REFERENCE)
            .sub(T(b"VMAD"), vmad)
            .sub(T(b"NAME"), 0x802u32.to_le_bytes())
            .sub(T(b"DATA"), [0u8; 24]),
    );
    let mut temporary = Group::new(GroupKind::CellTemporary(CELL));
    temporary.entries.push(Entry::Record(placed));
    let mut children = Group::new(GroupKind::CellChildren(CELL));
    children.entries.push(Entry::Group(temporary));
    let mut block = Group::new(GroupKind::InteriorSubBlock(0));
    block.entries.push(Entry::Record(cell_index));
    block.entries.push(Entry::Group(children));
    let mut interior = Group::new(GroupKind::InteriorBlock(0));
    interior.entries.push(Entry::Group(block));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(interior));
    plugin.groups.push(top);
    let mut order = LoadOrder::new();
    order
        .add("Scr05.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    game.load_script(&ScriptBuilder::new("Scr05Present", "ObjectReference").bytes())
        .unwrap();
    game
}

pub fn cell_id() -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: CELL,
    }
}

#[test]
fn scr_05_reference_scripts_wait_for_their_cell() {
    let mut game = fixture();
    let report = game.bind();
    assert_eq!(report.forms_with_scripts, 0);
    assert_eq!(report.scripts_attached, 0);

    let report = game.load_cell(cell_id());
    assert_eq!(report.forms_with_scripts, 1);
    assert_eq!(report.scripts_attached, 1);
    assert_eq!(
        report.scripts_missing,
        vec![(
            "Scr05Absent".into(),
            GlobalFormId {
                file: 0,
                object: REFERENCE
            }
        )]
    );
}

#[test]
fn scr_05_cell_script_binding_is_idempotent() {
    let mut game = fixture();
    game.load_cell(cell_id());
    let report = game.bind_cell(cell_id());
    assert_eq!(report, BindReport::default());
}

#[test]
fn scr_05_world_loaded_focus_binds_before_vm_work() {
    let mut game = fixture();
    game.bind();
    game.world_mut().load_cell(cell_id());
    game.run_until_idle(1);
    let object = game.object_of_form(GlobalFormId {
        file: 0,
        object: REFERENCE,
    });
    assert_eq!(game.vm.scripts_on(object), vec!["Scr05Present"]);
}

#[test]
fn scr_05_vanilla_missing_cell_attachments_are_reported() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the installed-data survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let path = data.join(name);
        let bytes =
            std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        let plugin = Plugin::parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        order.add(name, plugin).unwrap();
    }
    let cells: Vec<_> = order
        .winners_of(Tag::new(b"CELL"))
        .map(|(id, _)| id)
        .collect();
    let mut game = Game::new(order);
    game.load_scripts_from_data(&data);
    let mut missing = game.bind().scripts_missing;

    for cell in cells {
        missing.extend(game.load_cell(cell).scripts_missing);
    }
    let actual: std::collections::BTreeMap<_, _> = missing.iter().fold(
        std::collections::BTreeMap::new(),
        |mut counts, (name, _)| {
            *counts.entry(name.to_ascii_lowercase()).or_insert(0usize) += 1;
            counts
        },
    );
    let expected = std::collections::BTreeMap::from([
        ("dlc01soulcairnskullpuzzlescript".to_string(), 2),
        ("dlc1testphilatronach".to_string(), 1),
        ("dlc1testphilvortexscript".to_string(), 1),
        ("dlc1testphilvortextrigscript".to_string(), 1),
        ("dlc2benthiclurkerfxscript".to_string(), 1),
        ("draguremergescript".to_string(), 1),
        ("lokirsdraugrresurrection".to_string(), 2),
    ]);
    assert_eq!(actual, expected);
    assert_eq!(missing.len(), 9);
}
