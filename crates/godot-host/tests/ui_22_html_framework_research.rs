const UI_FRAMEWORKS: &str = include_str!("../../../docs/ui-frameworks.md");

#[test]
fn ui_22_embedding_options_compare_license_size_and_texture_support() {
    for required in [
        "Godot core has no HTML or browser renderer",
        "CEF through [gdCEF]",
        "System webview through [Godot WRY]",
        "Ultralight SDK",
        "about 1 GB of built artifacts",
        "No official runtime bundle size is published",
        "TextureRect",
        "always renders above Godot",
        "Godot texture",
    ] {
        assert!(
            UI_FRAMEWORKS.contains(required),
            "missing research clause: {required}"
        );
    }
}

#[test]
fn ui_22_documents_renderer_licenses_and_published_bundle_sizes() {
    for required in [
        "gdCEF is MIT",
        "CEF has LGPL third-party libraries",
        "The addon is MIT and WRY is MIT or Apache-2.0",
        "Ultralight's Free License Agreement",
        "does not publish a fixed bundle size",
        "No official runtime bundle size is published",
        "600 MB to download",
    ] {
        assert!(
            UI_FRAMEWORKS.contains(required),
            "missing license or size clause: {required}"
        );
    }
}

#[test]
fn ui_22_prismaui_api_documents_view_creation_and_bidirectional_events() {
    for required in [
        "RequestPluginAPI(InterfaceVersion::V1)",
        "creates one",
        "Data/PrismaUI/views/",
        "`Invoke`",
        "JavaScript listeners",
        "focus, visibility, ordering and destruction",
        "does not load an SKSE DLL",
    ] {
        assert!(
            UI_FRAMEWORKS.contains(required),
            "missing API clause: {required}"
        );
    }
}

#[test]
fn ui_22_recommendation_selects_an_optional_renderer_and_keeps_native_default() {
    for required in [
        "Keep native Godot controls as the default",
        "use gdCEF's",
        "behind an optional feature",
        "Measure memory, frame cost, input focus, transparency and texture upload",
        "Do not adopt WRY",
        "Revisit Ultralight only",
    ] {
        assert!(
            UI_FRAMEWORKS.contains(required),
            "missing recommendation: {required}"
        );
    }
}

#[test]
fn ui_22_proposes_contract_renderer_compatibility_and_sandbox_follow_ups() {
    for required in [
        "Specify the PrismaUI-compatible view and event contract",
        "Add an optional CEF renderer adapter",
        "Validate the adapter against named PrismaUI mods",
        "Survey HTML asset loading and script permissions",
    ] {
        assert!(
            UI_FRAMEWORKS.contains(required),
            "missing proposed row: {required}"
        );
    }
}
