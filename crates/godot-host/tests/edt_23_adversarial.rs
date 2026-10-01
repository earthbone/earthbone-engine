// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::facegen::{face_tint_path, FaceTintError};

#[test]
fn edt_23_tint_path_rejects_windows_parent_traversal() {
    assert!(matches!(
        face_tint_path(r"..\Example.esp", 0x1234),
        Err(FaceTintError::InvalidPluginName)
    ));
}

#[test]
fn edt_23_tint_path_rejects_form_ids_outside_the_local_record_range() {
    assert!(matches!(
        face_tint_path("Example.esp", 0x0100_0000),
        Err(FaceTintError::InvalidFormId)
    ));
}
