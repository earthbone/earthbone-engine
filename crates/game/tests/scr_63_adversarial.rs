// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_63_image_space_and_keyword_binding_kinds_match_scripts() {
    let game = Game::new(LoadOrder::new());
    for name in ["Apply", "ApplyCrossFade", "PopTo", "Remove"] {
        assert!(game.vm.natives.has_method("ImageSpaceModifier", name));
    }
    assert!(
        game.vm
            .natives
            .has_global("ImageSpaceModifier", "RemoveCrossFade")
    );
    for name in ["GetString", "SendStoryEvent", "SendStoryEventAndWait"] {
        assert!(game.vm.natives.has_method("Keyword", name));
    }
    assert!(
        !game
            .vm
            .natives
            .has_global("Keyword", "SendStoryEventAndWait")
    );
}
