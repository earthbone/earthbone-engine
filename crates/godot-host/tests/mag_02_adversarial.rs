// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_godot_host::combat::AimedProjectileContacts;

#[test]
fn mag_02_duplicate_contact_signal_for_one_projectile_is_ignored() {
    let mut contacts = AimedProjectileContacts::default();
    let mut hostile_hits = 0;

    assert!(contacts.contact(71, || {
        hostile_hits += 1;
        true
    }));
    assert!(!contacts.contact(71, || {
        hostile_hits += 1;
        true
    }));

    assert_eq!(hostile_hits, 1);
}

#[test]
fn mag_02_distinct_projectiles_can_hit_the_same_actor() {
    let mut contacts = AimedProjectileContacts::default();
    let mut hostile_hits = 0;

    for projectile_id in [71, 72] {
        assert!(contacts.contact(projectile_id, || {
            hostile_hits += 1;
            true
        }));
    }

    assert_eq!(hostile_hits, 2);
}

#[test]
fn mag_02_reused_instance_id_is_allowed_after_projectile_free() {
    let mut contacts = AimedProjectileContacts::default();
    assert!(contacts.contact(71, || true));
    assert!(!contacts.contact(71, || true));

    contacts.forget(71);

    assert!(contacts.contact(71, || true));
}

#[test]
fn mag_02_failed_contact_callback_is_not_retried_for_same_projectile() {
    let mut contacts = AimedProjectileContacts::default();
    let mut calls = 0;

    assert!(!contacts.contact(71, || {
        calls += 1;
        false
    }));
    assert!(!contacts.contact(71, || {
        calls += 1;
        true
    }));

    assert_eq!(calls, 1);
}
