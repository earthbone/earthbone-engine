// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed Whiterun flora and market items through free walk activation.

#[path = "common/isolated_project.rs"]
mod isolated_project;

#[test]
fn round4_freewalk_whiterun_harvest_and_market_inventory() -> Result<(), Box<dyn std::error::Error>>
{
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return Ok(());
    };
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        return Ok(());
    };
    let source = soe_godot_host::data_source::DataSource::open(std::path::Path::new(&data))?;
    let order = source.shared_order();
    let (city, _) = source
        .resolve_world("WhiterunWorld")
        .ok_or("WhiterunWorld is missing")?;
    let game = source.localized_game()?;
    let world = game.world();
    let mut samples = Vec::new();
    let mut counts = [0; 4];
    let mut references = std::collections::BTreeSet::new();
    for (cell, _) in source.cell_index().cells() {
        if source
            .cell_index()
            .grid_of(cell)
            .is_some_and(|grid| grid.world == city)
            || source.cell_index().world_persistent_cell(city) == Some(cell)
        {
            references.extend(source.cell_index().references(cell).iter().copied());
        }
    }
    for reference in references {
        if !soe_game::World::authored_reference_enabled(&order, reference) {
            continue;
        }
        let Some(winner) = order.winner_ref(reference) else {
            continue;
        };
        if order.record(winner).tag != soe_esm::Tag(*b"REFR") {
            continue;
        }
        let Ok(placed) = soe_esm::records::refr::Refr::from_record(order.record(winner)) else {
            continue;
        };
        let base = order.resolve(winner.file, placed.base);
        let Some(record) = order.winner(base) else {
            continue;
        };
        let harvest = world
            .harvest_data(base)?
            .filter(|data| data.chances == [100; 4]);
        let editor = record.editor_id().unwrap_or_default();
        let family = if harvest.is_some() {
            usize::from(record.tag.as_bytes() == b"TREE")
        } else if record.tag.as_bytes() == b"ALCH" && editor.to_ascii_lowercase().contains("goat") {
            2
        } else if record.tag.as_bytes() == b"INGR" {
            3
        } else {
            continue;
        };
        if counts[family] >= 2 {
            continue;
        }
        let produce = harvest.as_ref().map_or(base, |data| data.produce);
        if world.tag_of(produce) == Some(soe_esm::Tag(*b"LVLI")) {
            continue;
        }
        counts[family] += 1;
        let center = record
            .find(soe_esm::Tag(*b"OBND"))
            .filter(|bytes| bytes.len() == 12)
            .map(|bytes| {
                std::array::from_fn::<_, 3, _>(|axis| {
                    let lower = i16::from_le_bytes([bytes[axis * 2], bytes[axis * 2 + 1]]);
                    let upper = i16::from_le_bytes([bytes[axis * 2 + 6], bytes[axis * 2 + 7]]);
                    f32::from(lower).midpoint(f32::from(upper))
                })
            })
            .unwrap_or([0.0; 3]);
        samples.push(serde_json::json!({"file": reference.file, "object": reference.object, "base_file": produce.file, "base_object": produce.object, "harvest": harvest.is_some(), "input": family != 3, "center": center, "position": placed.transform.ok_or("sample has no placed transform")?.position, "name": editor, "display_name": world.name_of(reference).ok_or("sample has no display name")?, "produce_name": world.name_of(produce).ok_or("sample produce has no display name")?}));
    }
    assert!(
        counts[0] >= 1 && counts[1] >= 1 && counts[2] >= 1,
        "Whiterun harvest and goat samples: {counts:?}"
    );
    drop(world);
    drop(game);
    drop(order);
    drop(source);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let library = std::env::var_os("SOE_R4_TEST_LIBRARY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("target/release/libsoe_godot_host.so"));
    let project = isolated_project::project(
        &[
            "round4_harvest_take_test.gd",
            "ui/item_menu.gd",
            "ui/item_icon.gd",
        ],
        Some(&library),
    );
    let output = std::process::Command::new(godot)
        .args(["--headless", "--path"])
        .arg(project.path())
        .args(["--script", "round4_harvest_take_test.gd"])
        .env("SKYRIM_DATA", data)
        .env("SOE_R4_HARVEST_TAKE", serde_json::to_string(&samples)?)
        .env("SOE_THREADED_SIMULATION", "0")
        .env_remove("SOE_CAPTURE_PATH")
        .env_remove("SOE_AUD_11_TEST")
        .output()?;
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let log_name = if library
        .file_name()
        .is_some_and(|name| name == "soe-r4-before-gmst.so")
    {
        "target/round4-harvest-label-before.log"
    } else if std::env::var_os("SOE_R4_TEST_LIBRARY").is_some() {
        "target/round4-harvest-take-before.log"
    } else {
        "target/round4-harvest-take.log"
    };
    std::fs::write(root.join(log_name), &log)?;
    let report = log
        .lines()
        .filter(|line| line.contains("round4_harvest_take") || line.contains("SCRIPT ERROR"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output.status.success()
            && log.contains("PASS round4_harvest_take")
            && !log.contains("SCRIPT ERROR"),
        "exit {:?}: {report}",
        output.status.code()
    );
    eprintln!("{report}");
    Ok(())
}
