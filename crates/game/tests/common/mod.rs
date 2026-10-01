// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

//! Shared integration fixtures from installed masters and the SOEW save reader.

pub(crate) mod installed;
pub(crate) mod legacy;
pub(crate) mod mag_adversarial;
pub(crate) mod save_tail;

/// Return a libtest name without the binary's crate-name prefix.
pub(crate) fn child_test_name(module: &str, test: &str) -> String {
    module.split_once("::").map_or_else(
        || test.to_owned(),
        |(_, module)| format!("{module}::{test}"),
    )
}
