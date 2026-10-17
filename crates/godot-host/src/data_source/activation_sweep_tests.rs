// SPDX-License-Identifier: GPL-3.0-or-later

//! Dwemer switch coverage from installed ACTI models and their authored animation.

use super::*;

#[test]
fn activation_sweep_dwemer_switch_models_have_executable_surfaces() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(Path::new(&data)).unwrap();
    let mut checked = 0;
    let mut missing = Vec::new();
    for (_, record) in source.order.winners_of(Tag(*b"ACTI")) {
        let name = record.editor_id().unwrap_or_default();
        let lower = name.to_ascii_lowercase();
        if !lower.contains("dwe") || !(lower.contains("button") || lower.contains("lever")) {
            continue;
        }
        let Some(raw) = record.find(Tag(*b"MODL")) else {
            continue;
        };
        let path = String::from_utf8_lossy(raw)
            .trim_end_matches('\0')
            .to_string();
        let surfaces = source.animated_model_surfaces(&path).unwrap();
        checked += 1;
        if !surfaces.iter().any(|surface| surface.animation.is_some()) {
            missing.push((name, path));
        }
    }
    assert!(
        checked >= 2,
        "installed Dwemer button and lever bases must be covered"
    );
    assert!(
        missing.is_empty(),
        "frozen Dwemer switch models: {missing:?}"
    );
}
