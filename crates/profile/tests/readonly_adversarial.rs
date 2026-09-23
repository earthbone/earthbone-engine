// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for `Loadout::package` against read-only source
//! files, TOOL-02. `copy_file_preserving` in `crates/profile/src/lib.rs`
//! says it keeps "the source's permission bits (`fs::copy` already does this
//! on Unix)", and `copy_tree`'s own doc comment promises "a file already at
//! the destination is overwritten, so calling this for each data root in low
//! to high order reproduces the shadowing `Loadout::find` does". Read-only
//! mod files are ordinary (Nexus downloads, BSA extraction and read-only
//! media all leave files without the owner-write bit), and the packaging
//! open question in the task pack itself asks about this case. These tests
//! check the two contracts hold together: a later root must still be able
//! to shadow a read-only file from an earlier one, and packaging the same
//! loadout to the same destination twice, the way a manager would refresh a
//! deployed profile, must still succeed.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use soe_profile::LoadoutInput;

fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn make_read_only(path: &Path) {
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o444);
    std::fs::set_permissions(path, perms).unwrap();
}

/// Sourced from `copy_tree`'s doc comment: "A file already at the
/// destination is overwritten, so calling this for each data root in low to
/// high order reproduces the shadowing `Loadout::find` does." The low root's
/// file here is read-only, which `copy_file_preserving` carries onto the
/// packaged copy per its own doc comment. The high root must still win.
#[test]
fn package_shadows_a_read_only_lower_roots_file() {
    let tmp = tempfile::tempdir().unwrap();
    let low = tmp.path().join("low");
    let high = tmp.path().join("high");
    write(&low.join("textures/a.dds"), b"low content");
    make_read_only(&low.join("textures/a.dds"));
    write(&high.join("textures/a.dds"), b"high content");

    let input = LoadoutInput {
        game_dir: tmp.path().join("game"),
        data_roots: vec![low, high],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    };
    let loadout = input.resolve().unwrap();
    let dest = tmp.path().join("packaged");
    let result = loadout.package(&dest);
    assert!(
        result.is_ok(),
        "a read-only file at the shadowed path must not abort packaging: {result:?}"
    );
    let packaged = std::fs::read(dest.join("Data/textures/a.dds")).unwrap();
    assert_eq!(
        packaged, b"high content",
        "the higher root must still win over a read-only lower root file"
    );
}

/// Sourced from TOOL-02's own done check, "a resolved loadout runs without a
/// mod manager in the loop": a manager refreshes a deployed profile by
/// packaging again, the same way MO2 redeploys on every launch. Packaging
/// the same loadout to the same destination twice must not fail just
/// because the first pass left a read-only file where the second pass needs
/// to write again.
#[test]
fn package_can_repackage_over_its_own_read_only_output() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("root");
    write(&root.join("textures/a.dds"), b"content");
    make_read_only(&root.join("textures/a.dds"));

    let input = LoadoutInput {
        game_dir: tmp.path().join("game"),
        data_roots: vec![root],
        plugins_txt: None,
        loadorder_txt: None,
        ini_dir: None,
        ccc: None,
    };
    let loadout = input.resolve().unwrap();
    let dest = tmp.path().join("packaged");
    loadout.package(&dest).unwrap();

    let result = loadout.package(&dest);
    assert!(
        result.is_ok(),
        "repackaging the same loadout to the same destination must not fail \
         on a read-only file the first pass wrote: {result:?}"
    );
}
