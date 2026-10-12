// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use soe_trace::{Entity, Profile};
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

#[test]
fn prf_13_nonfinite_and_negative_waits_are_zero_and_keep_wait_counts() {
    let _guard = LOCK.lock().unwrap();
    Profile::disable();
    Profile::clear();
    Profile::enable();
    let script = || Entity::new("script", "Waiter.Run", "Waiter.esp", "latent_wait");
    for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
        Profile::wait(script, seconds, 1);
    }
    Profile::disable();

    let rows = Profile::summary();
    let stats = &rows[&Entity::new("script", "Waiter.Run", "Waiter.esp", "latent_wait")];
    assert_eq!(stats.waits, 4);
    assert_eq!(stats.wait_ns, 0);
    assert_eq!(Profile::ranking(&rows, "plugin")[0].0, "Waiter.esp");
}
