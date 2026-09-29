// SPDX-License-Identifier: GPL-3.0-or-later

//! Routes newly recorded game animation events to the host actor with the
//! matching form identity.

use soe_esm::GlobalFormId;

/// Position in the world's append-only animation event log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationEventCursor(usize);

impl AnimationEventCursor {
    /// Start before the first event, useful when the consumer owns the full log.
    pub const fn beginning() -> Self {
        Self(0)
    }

    /// Start after events already present when a host actor is bound.
    pub const fn at_end(events: &[(GlobalFormId, String)]) -> Self {
        Self(events.len())
    }

    /// Return new requests for `actor` and advance past every observed event.
    pub fn take_for(
        &mut self,
        events: &[(GlobalFormId, String)],
        actor: GlobalFormId,
    ) -> Vec<String> {
        let start = self.0.min(events.len());
        self.0 = events.len();
        events[start..]
            .iter()
            .filter(|(source, _)| *source == actor)
            .map(|(_, name)| name.clone())
            .collect()
    }
}

/// Report and consume graph events returned by one interpreter update.
pub fn report_completed_events(events: &mut Vec<String>, mut report: impl FnMut(&str)) {
    for event in events.drain(..) {
        report(&event);
    }
}
