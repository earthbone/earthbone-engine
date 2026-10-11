// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_commonlib_shim::events::{request_hook, HookError, HookKind};

#[test]
fn shm_21_all_binary_hook_kinds_return_explicit_unsupported_errors() {
    for kind in [HookKind::CallSite, HookKind::Branch, HookKind::Vtable] {
        assert_eq!(request_hook(kind), Err(HookError::Unsupported(kind)));
    }
}
