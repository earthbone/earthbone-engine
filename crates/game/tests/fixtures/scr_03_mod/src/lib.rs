wit_bindgen::generate!({
    path: "../../../wit",
    world: "runtime",
});

struct Mod;

impl Guest for Mod {
    fn initialize() {
        soe::mod_api::engine::subscribe_on_activate();
    }

    fn on_activate(
        _target_file: u32,
        _target_object: u32,
        _activator_file: u32,
        _activator_object: u32,
    ) {
        soe::mod_api::engine::set_stage(0, 0x800, 10);
    }
}

export!(Mod);
