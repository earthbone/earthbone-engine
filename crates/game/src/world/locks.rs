// SPDX-License-Identifier: GPL-3.0-or-later

//! Lock state shared by native calls and default activation. Contracts follow
//! CK Lock, GetLockLevel and OnLockStateChanged, plus the shipped
//! ObjectReference.psc GetKey and SetLockLevel declarations.

use soe_esm::records::refr::Lock;
use soe_esm::GlobalFormId;

use super::{Alloc, Handle, Pending, World, T};

impl World {
    /// The authored key stays in the winning REFR, not a second runtime table.
    pub fn key(&self, reference: GlobalFormId) -> Option<GlobalFormId> {
        let owner = self.order.winner_ref(reference)?;
        let record = self.order.record(owner);
        if record.tag != T(b"REFR") || record.is_deleted() {
            return None;
        }
        let lock = Lock::from_bytes(record.find(T(b"XLOC"))?).ok()?;
        if lock.key == 0 {
            return None;
        }
        let key = self.order.resolve(owner.file, lock.key);
        self.order
            .winner(key)
            .filter(|r| r.tag == T(b"KEYM") && !r.is_deleted())
            .map(|_| key)
    }

    /// Lock always emits OnLockStateChanged, including repeated calls. Locking
    /// an open object snaps its logical open state closed, as the CK specifies.
    pub fn set_locked(&mut self, reference: GlobalFormId, locked: bool, alloc: Alloc<'_>) {
        let Some(state) = self.reference(reference) else {
            return;
        };
        state.locked = locked;
        if locked {
            state.opened = false;
        }
        self.lock_event(reference, alloc);
    }

    /// Preserve locked/unlocked state while changing the lock's byte-range
    /// difficulty. Creating a lock on an unlocked reference does not lock it.
    pub fn set_lock_level(&mut self, reference: GlobalFormId, level: i32, alloc: Alloc<'_>) {
        let Some(state) = self.reference(reference) else {
            return;
        };
        let level = level.clamp(0, 255);
        if state.lock_level != level {
            state.lock_level = level;
            self.lock_event(reference, alloc);
        }
    }

    fn lock_event(&mut self, reference: GlobalFormId, alloc: Alloc<'_>) {
        let object = self.object_of(Handle::Form(reference), alloc);
        self.pending.push(Pending::Event {
            object,
            name: "OnLockStateChanged".into(),
            args: Vec::new(),
        });
    }

    /// Default activation uses an available key without consuming it. A failed
    /// attempt leaves both the lock and inventory unchanged for lockpicking UI.
    pub(super) fn unlock_with_key(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> bool {
        if !self.reference(target).is_some_and(|r| r.locked) {
            return true;
        }
        let Some(key) = self.key(target) else {
            return false;
        };
        if self.item_count(activator, key) <= 0 {
            return false;
        }
        self.set_locked(target, false, alloc);
        true
    }
}
