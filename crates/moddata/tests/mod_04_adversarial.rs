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
fn mod_04_user_priority_moves_author_submod_ahead_of_competitor() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "OAR/Pack/Author/config.json",
        r#"{"name":"Author","priority":4}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Author/user.json",
        r#"{"priority":10}"#,
    );
    write(
        root.path(),
        "OAR/Pack/Other/config.json",
        r#"{"name":"Other","priority":8}"#,
    );

    let scanned = AnimationMod::scan(root.path()).unwrap();
    assert_eq!(scanned.sub_mods[0].name, "Author");
    assert_eq!(scanned.sub_mods[0].priority, 10);
}

#[test]
fn mod_04_user_conditions_replace_config_even_when_user_conditions_are_missing() {
    let root = tempfile::tempdir().unwrap();
    write(
        root.path(),
        "OAR/Pack/Walk/config.json",
        r#"{"name":"Walk","priority":4,"conditions":[{"condition":"AuthorOnly"}]}"#,
    );
    write(root.path(), "OAR/Pack/Walk/user.json", r#"{"priority":8}"#);

    let scanned = AnimationMod::scan(root.path()).unwrap();
    assert_eq!(scanned.sub_mods[0].conditions.len(), 0);
}
