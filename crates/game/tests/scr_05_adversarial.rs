// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod acceptance {
    include!("scr_05_lazy_cell_binding.rs");

    #[cfg(test)]
    mod adversarial {
        use super::{cell_id, fixture, REFERENCE};
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
            assert_eq!(game.vm.scripts_on(game.object_of_form(GlobalFormId { file: 0, object: REFERENCE })), vec!["Scr05Present"]);
        }
    }
}
