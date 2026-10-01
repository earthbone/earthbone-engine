// SPDX-License-Identifier: MIT
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::refr::ObjectRefHandle;
use soe_commonlib_shim::world::{Projectile, ProjectileHandle, Unsupported};

#[test]
fn opaque_handles_preserve_all_32_bits_and_projectile_runtime_rejects_repeatably() {
    let raw = u32::MAX;
    assert_eq!(ObjectRefHandle(raw).0, raw);
    let projectile = Projectile {
        handle: ProjectileHandle(raw),
    };
    assert_eq!(projectile.handle.0, raw);
    assert_eq!(projectile.runtime(), Err(Unsupported::ProjectileRuntime));
    assert_eq!(projectile.runtime(), Err(Unsupported::ProjectileRuntime));
}
