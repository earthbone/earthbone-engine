// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::combat::AimedProjectileContacts;

#[test]
fn mag_02_one_projectile_contact_is_dispatched_once() {
    let mut contacts = AimedProjectileContacts::default();
    let mut casts = 0;

    assert!(contacts.contact(41, || {
        casts += 1;
        true
    }));
    assert!(!contacts.contact(41, || {
        casts += 1;
        true
    }));

    assert_eq!(casts, 1);
}

#[test]
fn mag_02_freed_projectile_releases_its_contact_guard() {
    let mut contacts = AimedProjectileContacts::default();
    assert!(contacts.contact(41, || true));
    contacts.forget(41);
    assert!(contacts.contact(41, || true));
}
