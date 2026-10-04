// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed-data survey for the form signatures that carry scene models.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::*;

fn neutral_reference() -> Refr {
    Refr {
        form_id: 0,
        record_flags: 0,
        editor_id: None,
        base: 0,
        transform: None,
        scale: 1.0,
        enable_parent: None,
        owner: None,
        count: None,
        teleport: None,
        lock: None,
        location: None,
        layer: None,
        encounter_zone: None,
        emittance: None,
        linked_refs: Vec::new(),
        primitive: None,
        cell: None,
        world: None,
        in_persistent_group: false,
    }
}

#[test]
fn installed_winning_model_signatures_keep_editor_markers_out_of_draws() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping the installed marker survey");
        return;
    };
    let source = DataSource::open_with_threads(&data, 1).expect("open installed data");
    let mut model_bearing_signatures = BTreeMap::new();
    let mut selected_signatures = BTreeMap::new();
    let mut paths = BTreeSet::new();
    for (_, winner) in source.order.winners() {
        let record = source.order.record(winner);
        if record
            .find(soe_esm::Tag::new(b"MODL"))
            .map(soe_esm::cursor::zstring)
            .is_some_and(|model| !model.is_empty())
        {
            *model_bearing_signatures.entry(record.tag).or_insert(0usize) += 1;
        }
        let Some(model) = models::render_model_for(record) else {
            continue;
        };
        *selected_signatures.entry(record.tag).or_insert(0usize) += 1;
        paths.insert(model);
    }

    let marker_candidates: Vec<_> = paths
        .iter()
        .filter(|model| model.to_ascii_lowercase().contains("marker"))
        .cloned()
        .collect();
    let ordinary_controls: Vec<_> = paths
        .iter()
        .filter(|model| !model.to_ascii_lowercase().contains("marker"))
        .take(25)
        .cloned()
        .collect();
    let mut before = 0usize;
    let mut filtered = 0usize;
    let mut marker_containing_models = 0usize;
    let mut marker_only_models = 0usize;
    let mut ordinary_visible_meshes = 0usize;
    let mut failures = Vec::new();
    for model in marker_candidates.iter().chain(&ordinary_controls) {
        let path = format!("meshes/{model}");
        let result = source
            .vfs
            .read(&path)
            .map_err(DataSourceError::from)
            .and_then(|bytes| placed_model(&bytes));
        let file = match result {
            Ok(file) => file,
            Err(error) => {
                failures.push(format!("{path}: {error}"));
                continue;
            }
        };
        let meshes = file.meshes().expect("parsed NIF meshes");
        let marker_blocks = editor_marker_shape_blocks(&file);
        let nonvisual_blocks = nonvisual_shape_indices(&file.blocks);
        let marker_shapes = meshes
            .iter()
            .filter(|mesh| marker_blocks.contains(&mesh.block))
            .count();
        before += marker_shapes;
        filtered += marker_shapes;
        let expected: Vec<_> = meshes
            .iter()
            .filter(|mesh| {
                !marker_blocks.contains(&mesh.block) && !nonvisual_blocks.contains(&mesh.block)
            })
            .collect();
        let actual = source
            .load_static_surfaces(model, &neutral_reference(), false)
            .expect("production model load");
        let LoadedStatic::Surfaces { built, .. } = actual;
        assert_eq!(
            built.len(),
            expected.len(),
            "{model}: marker shape reached rendering"
        );
        assert_eq!(
            built
                .iter()
                .map(|(arrays, _)| arrays.triangle_count())
                .sum::<usize>(),
            expected
                .iter()
                .map(|mesh| mesh.triangle_count())
                .sum::<usize>(),
            "{model}: production filter changed nonmarker triangles"
        );
        if marker_shapes > 0 {
            marker_containing_models += 1;
            marker_only_models += usize::from(marker_shapes == meshes.len());
        } else if !meshes.is_empty() {
            ordinary_visible_meshes += 1;
        }
    }

    let rendered_markers = before - filtered;
    println!("all model-bearing winning signatures:");
    for (signature, count) in model_bearing_signatures {
        println!("  {signature}: {count}");
    }
    println!("scene-model signatures selected by the host:");
    for (signature, count) in selected_signatures {
        println!("  {signature}: {count}");
    }
    println!("editor-marker shapes before filtering: {before}");
    println!("editor-marker shapes filtered: {filtered}");
    println!("marker-containing models: {marker_containing_models}");
    println!("marker-only models: {marker_only_models}");
    println!("editor-marker shapes drawn after filtering: {rendered_markers}");
    println!("ordinary visible model paths retained: {ordinary_visible_meshes}");
    println!("model paths that failed to inspect: {}", failures.len());
    for failure in failures.iter().take(10) {
        println!("  failed: {failure}");
    }

    assert!(marker_containing_models > 0, "no marker model was found");
    assert!(
        ordinary_visible_meshes >= 10,
        "marker filtering retained too few ordinary visible meshes"
    );
    assert_eq!(rendered_markers, 0, "an editor marker remained drawable");
}
