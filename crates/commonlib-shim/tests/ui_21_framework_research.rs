const RESEARCH: &str = include_str!("../../../docs/ui-frameworks.md");

#[test]
fn ui_21_documents_skse_and_dmenu_public_api_surfaces() {
    for api in [
        "`SetSection(name)`",
        "`AddSectionItem(name, render_callback)`",
        "`AddWindow(render_callback, pause_game)`",
        "`AddInputEvent`",
        "`dMenu_OpenMenu`",
        "`dMenu_GetInterface(version)`",
    ] {
        assert!(RESEARCH.contains(api), "research must document {api}");
    }
}

#[test]
fn ui_21_documents_mit_boundary_and_piece_licenses() {
    for required in [
        "## What belongs in the MIT shim",
        "It must not contain Dear ImGui, rendering code, a port of either",
        "| dMenu NG implementation | GPL-3.0-or-later",
        "| `imgui-godot` | MIT",
        "| Rust `imgui-rs` | MIT or Apache-2.0",
        "## License findings",
    ] {
        assert!(
            RESEARCH.contains(required),
            "research must include {required}"
        );
    }
}

#[test]
fn ui_21_documents_godot_render_options_and_follow_up_rows() {
    for required in [
        "## ImGui inside the Godot host",
        "`imgui-godot` by pkdawson",
        "Rust `imgui` (`imgui-rs`) with a host-owned renderer",
        "Recommendation: prototype the Rust `imgui` path in `godot-host`.",
        "| UI-27 Define menu framework ABI |",
        "| UI-28 Render registered ImGui pages in Godot |",
        "| UI-29 Execute SKSE Menu Framework style pages |",
        "| UI-30 Load dMenu NG custom settings definitions |",
    ] {
        assert!(
            RESEARCH.contains(required),
            "research must include {required}"
        );
    }
}
