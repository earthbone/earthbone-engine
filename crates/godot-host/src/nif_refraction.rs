// SPDX-License-Identifier: GPL-3.0-or-later

//! Lighting refraction follows OpenMW nifosg/nifloader.cpp and
//! shaders/lib/util/distortion.glsl. Normal-map RG displaces the screen,
//! masked by texture alpha and the authored strength. OpenMW's final
//! internal_distortion.omwfx multiplies that displacement by 0.14.

use crate::mesh::MaterialDesc;
use soe_nif::blocks::flags1;

/// The distortion pass precedes ordinary transparent flame layers.
pub fn shader(desc: &MaterialDesc) -> String {
    let cull = if desc.double_sided {
        "cull_disabled"
    } else {
        "cull_back"
    };
    let depth_test = if desc.shader_flags_1 & flags1::ZBUFFER_TEST == 0 {
        ", depth_test_disabled"
    } else {
        ""
    };
    include_str!("nif_refraction.gdshader")
        .replace("$CULL", cull)
        .replace("$DEPTH_TEST", depth_test)
        .replace(
            "$VERTEX_ALPHA",
            if desc.shader_flags_1 & flags1::VERTEX_ALPHA != 0 {
                "COLOR.a"
            } else {
                "1.0"
            },
        )
}
