// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_moddata::oar::AnimationMod;
use std::fs;

fn write(root: &std::path::Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn mod_04_negative_user_priority_loses_to_default_zero_submod() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "OAR/Pack/Author/config.json",
        r#"{"name":"Author","priority":20}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Author/user.json",
        r#"{"priority":-1}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Other/config.json",
        r#"{"name":"Other","priority":0}"#,
    );

    let scanned = AnimationMod::scan(root.path()).unwrap();
    let selected = scanned.select_submod(|_| true).unwrap();
    assert_eq!(selected.name, "Other");
    assert_eq!(selected.priority, 0);
}

#[test]
fn mod_04_i32_max_user_priority_wins_selection() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "OAR/Pack/Author/config.json",
        r#"{"name":"Author","priority":0}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Author/user.json",
        r#"{"priority":2147483647}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Other/config.json",
        r#"{"name":"Other","priority":2147483646}"#,
    );

    let scanned = AnimationMod::scan(root.path()).unwrap();
    let selected = scanned.select_submod(|_| true).unwrap();
    assert_eq!(selected.name, "Author");
    assert_eq!(selected.priority, i32::MAX);
}

#[test]
fn mod_04_user_priority_does_not_fall_back_to_author_priority_when_missing() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "OAR/Pack/Author/config.json",
        r#"{"name":"Author","priority":20}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Author/user.json",
        r#"{"conditions":[]}"#,
    );

    let scanned = AnimationMod::scan(root.path()).unwrap();
    assert!(scanned.sub_mods.is_empty());
}

#[test]
fn mod_04_user_priority_rejects_values_outside_i32_range() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "OAR/Pack/Author/config.json",
        r#"{"name":"Author","priority":20}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Author/user.json",
        r#"{"priority":2147483648}"#,
    );

    let scanned = AnimationMod::scan(root.path()).unwrap();
    assert!(scanned.sub_mods.is_empty());
}
