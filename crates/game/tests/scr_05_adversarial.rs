// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "scr_05_lazy_cell_binding.rs"]
mod acceptance;

#[cfg(test)]
mod adversarial {
    use super::acceptance::{REFERENCE, cell_id, fixture};
    use soe_esm::GlobalFormId;

    #[test]
    fn scr_05_explicit_binding_before_load_does_not_attach_early() {
        let mut game = fixture();
        let premature = game.bind_cell(cell_id());
        assert_eq!(premature.forms_with_scripts, 0);
        assert_eq!(premature.scripts_attached, 0);

        let loaded = game.load_cell(cell_id());
        assert_eq!(loaded.forms_with_scripts, 1);
        assert_eq!(loaded.scripts_attached, 1);
        let object = game.object_of_form(GlobalFormId {
            file: 0,
            object: REFERENCE,
        });
        assert_eq!(game.vm.scripts_on(object), vec!["Scr05Present"]);
    }
}
