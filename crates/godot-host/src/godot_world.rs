// SPDX-License-Identifier: GPL-3.0-or-later

//! Rendering scenario lookup for host nodes. The root viewport fallback is
//! needed by Godot SceneTree scripts that load resources during initialization.

use godot::classes::{Engine, Node3D, SceneTree, Viewport};
use godot::prelude::*;

pub(crate) fn scenario(node: &Node3D, owner: &str) -> Option<Rid> {
    let own = node
        .get_viewport()
        .and_then(|viewport| viewport.find_world_3d());
    let world = own.or_else(|| {
        let tree = Engine::singleton()
            .get_main_loop()?
            .try_cast::<SceneTree>()
            .ok()?;
        tree.get_root()?.upcast::<Viewport>().find_world_3d()
    });
    if world.is_none() {
        soe_trace::error!(target: "host::world", system = "host::world", "{owner}: no World3D to draw into");
    }
    world.map(|w| w.get_scenario())
}
