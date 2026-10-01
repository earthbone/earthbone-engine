// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

use soe_esm::LoadOrder;
use soe_game::Game;

#[test]
fn scr_71_spell_and_topic_bindings_are_instance_methods() {
    let mut game = Game::new(LoadOrder::new());
    game.bind();

    let expected_methods = [
        ("spell", "cast"),
        ("spell", "ishostile"),
        ("spell", "preload"),
        ("spell", "remotecast"),
        ("spell", "unload"),
        ("topic", "add"),
    ];
    let methods: Vec<_> = game
        .vm
        .natives
        .method_bindings()
        .into_iter()
        .filter(|(class, _)| class == "spell" || class == "topic")
        .collect();
    let globals: Vec<_> = game
        .vm
        .natives
        .global_bindings()
        .into_iter()
        .filter(|(class, _)| class == "spell" || class == "topic")
        .collect();
    assert_eq!(
        methods,
        expected_methods.map(|(class, name)| (class.to_owned(), name.to_owned()))
    );
    assert!(
        globals.is_empty(),
        "unexpected Spell or Topic globals: {globals:?}"
    );

    for (class, name) in [
        ("Spell", "Cast"),
        ("Spell", "IsHostile"),
        ("Spell", "RemoteCast"),
        ("Spell", "Preload"),
        ("Spell", "Unload"),
        ("Topic", "Add"),
    ] {
        assert!(game.vm.natives.has_method(class, name), "{class}.{name}");
        assert!(!game.vm.natives.has_global(class, name), "{class}.{name}");
    }
    assert!(!game.vm.natives.has_method("Topic", "Remove"));
}
