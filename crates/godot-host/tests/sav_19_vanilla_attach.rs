// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UESP player location and FallrimTools change-form identities are checked
//! against atomic ESS recovery, then the production host attachment path.

use std::{fs, path::Path, path::PathBuf, process::Command};

use serde_json::json;
use soe_game::{save_generations::GenerationStore, vanilla_save::SavedFormMap};
use soe_godot_host::{data_source::DataSource, save_recovery::accept_vanilla_recovery_session};
use soe_save::Save;

fn saves_under(directory: &Path, paths: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            saves_under(&path, paths);
        } else if path.extension().is_some_and(|extension| extension == "ess") {
            paths.push(path);
        }
    }
}

#[test]
fn sav_19_vanilla_atomic_import_attaches_saved_cell_pose_camera_and_verified_frame() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("skipping: set SKYRIM_DATA");
        return;
    };
    let Some(corpus) = std::env::var_os("SOE_TEST_SAVES").map(PathBuf::from) else {
        eprintln!("skipping: set SOE_TEST_SAVES");
        return;
    };
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output_dir = workspace.join("target/sav_19");
    fs::create_dir_all(&output_dir).unwrap();
    let source = DataSource::open(&data).unwrap();
    let mut game = source.localized_game().unwrap();
    let scripts = game.load_scripts_from_data(&data);
    assert!(
        scripts.scripts_failed.is_empty(),
        "{:?}",
        scripts.scripts_failed
    );
    let pristine = game.save_state().unwrap();
    let mut paths = Vec::new();
    saves_under(&corpus, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "save corpus is empty");
    let mut survey = Vec::new();
    let mut selected = None;
    let mut selected_interior = false;
    let mut clean_successes = 0;
    let mut accepted_total = 0;
    for path in &paths {
        game.load_state(&pristine).unwrap();
        let original = fs::read(path).unwrap();
        let save = Save::parse(&original).unwrap();
        let directory = tempfile::tempdir_in(&output_dir).unwrap();
        let store = GenerationStore::new(directory.path(), 2).unwrap();
        let name = path.strip_prefix(&corpus).unwrap().to_string_lossy();
        match accept_vanilla_recovery_session(&mut game, &save, &store) {
            Ok(_accepted) => {
                let player_id = game.player();
                let world = game.world();
                let player = world.reference_ref(player_id).unwrap();
                let location = save.player_location().unwrap().unwrap();
                let mapping = SavedFormMap::new(&save, world.order()).unwrap();
                let header = save
                    .change_forms
                    .iter()
                    .find_map(|change| {
                        (mapping.reference(change.form).unwrap() == Some(player_id))
                            .then(|| change.reference_header(save.form_version).unwrap().unwrap())
                    })
                    .unwrap();
                let placement = header.initial.placement().unwrap();
                assert_eq!(player.position, location.position);
                assert_eq!(player.rotation, placement.rotation);
                let parent = mapping.reference(location.cell_or_world).unwrap().unwrap();
                let expected_cell =
                    if world.order().winner(parent).unwrap().tag == soe_esm::Tag(*b"CELL") {
                        parent
                    } else {
                        source
                            .cell_index()
                            .exterior_cell(parent, location.coordinates[0], location.coordinates[1])
                            .unwrap()
                    };
                assert_eq!(player.cell, Some(expected_cell));
                let clean = path
                    .strip_prefix(&corpus)
                    .unwrap()
                    .starts_with("Clean Saves");
                clean_successes += usize::from(clean);
                accepted_total += 1;
                let interior = source.cell_index().is_interior(expected_cell);
                survey.push(json!({"save": name, "ok": true}));
                if clean && (selected.is_none() || (interior && !selected_interior)) {
                    let expected = json!({
                        "cell": expected_cell.to_string(),
                        "position": player.position,
                        "rotation": player.rotation,
                        "days": world.clock.game_days,
                        "save": name,
                    });
                    selected = Some((directory, expected));
                    // CONTEXT: Prefer an interior for bounded streaming and a
                    // visible saved view, while surveying every save afterward.
                    selected_interior = interior;
                }
            }
            Err(error) => {
                eprintln!("SAV-19 {name}: {error}");
                assert_eq!(
                    game.save_state().unwrap(),
                    pristine,
                    "refusal changed {name}"
                );
                survey.push(json!({"save": name, "ok": false, "error": error.to_string()}));
            }
        }
        assert_eq!(fs::read(path).unwrap(), original, "source ESS changed");
    }
    fs::write(
        output_dir.join("survey.json"),
        serde_json::to_vec_pretty(&survey).unwrap(),
    )
    .unwrap();
    eprintln!(
        "SAV-19 surveyed {} saves, {clean_successes} Clean Saves accepted",
        paths.len()
    );
    assert!(
        clean_successes > 0,
        "no Clean Save recovered, see target/sav_19/survey.json"
    );
    // The supplied corpus has 60 saves. 57 import, 18 of them Clean Saves, and
    // the three refusals are named in docs/godot-host.md. A smaller corpus
    // only has to satisfy the check above.
    if paths.len() == 60 {
        assert!(
            accepted_total >= 57 && clean_successes >= 18,
            "imported {accepted_total} of 60 saves ({clean_successes} Clean Saves), \
             expected at least 57 and 18, see target/sav_19/survey.json"
        );
    }
    let (directory, expected) = selected.unwrap();
    let expected_path = output_dir.join("expected.json");
    fs::write(
        &expected_path,
        serde_json::to_vec_pretty(&expected).unwrap(),
    )
    .unwrap();
    let Some(godot) = std::env::var_os("SOE_GODOT_BIN") else {
        eprintln!("skipping host capture: set SOE_GODOT_BIN");
        return;
    };
    let output = Command::new(godot)
        .arg("--path")
        .arg(workspace.join("godot"))
        .args(["--resolution", "640x360", "--script"])
        .arg(workspace.join("godot/sav_19_vanilla_attach_test.gd"))
        .env("SKYRIM_DATA", data)
        .env("SAV_19_SAVE_DIR", directory.path())
        .env("SAV_19_EXPECTED", &expected_path)
        .env("SAV_19_CAPTURE", output_dir.join("frame.png"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    fs::write(output_dir.join("host.log"), format!("{stdout}\n{stderr}")).unwrap();
    assert!(
        output.status.success() && stdout.contains("RESULT: PASS sav_19 vanilla attach"),
        "host attachment failed\n{stdout}\n{stderr}"
    );
}
