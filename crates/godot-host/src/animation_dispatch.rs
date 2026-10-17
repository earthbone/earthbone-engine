// SPDX-License-Identifier: GPL-3.0-or-later

//! Routes game animation requests to the host visual with the matching form
//! identity.

use soe_esm::GlobalFormId;

/// Last posture submitted to one graph instance, separate from game state.
#[cfg(any(feature = "godot", test))]
#[derive(Debug, Default)]
pub(crate) struct SneakGraphState {
    sneaking: bool,
}

#[cfg(any(feature = "godot", test))]
impl SneakGraphState {
    /// Publish the shared actor posture and raise only its transition edge.
    /// Event names follow the vanilla master behavior's sneak transitions.
    pub(crate) fn apply(&mut self, vm: &mut soe_hkx::interp::Interpreter<'_>, sneaking: bool) {
        if let Some(event) = self.transition(sneaking) {
            vm.send_event_named(event);
        }
    }

    fn transition(&mut self, sneaking: bool) -> Option<&'static str> {
        if self.sneaking == sneaking {
            return None;
        }
        self.sneaking = sneaking;
        Some(if sneaking { "SneakStart" } else { "SneakStop" })
    }
}

/// Position in the world's append-only animation request log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationEventCursor(usize);

impl AnimationEventCursor {
    /// Start before the first event, useful when the consumer owns the full log.
    pub const fn beginning() -> Self {
        Self(0)
    }

    /// Start after events already present when a host actor is bound.
    pub const fn at_end(requests: &[soe_game::world::AnimationRequest]) -> Self {
        Self(requests.len())
    }

    /// Return new requests for `target` and advance past every observed one.
    pub fn take_for(
        &mut self,
        requests: &[soe_game::world::AnimationRequest],
        target: GlobalFormId,
    ) -> Vec<String> {
        let start = self.0.min(requests.len());
        self.0 = requests.len();
        requests[start..]
            .iter()
            .filter(|request| request.target == target)
            .map(|request| request.animation.clone())
            .collect()
    }
}

/// Report and consume graph events returned by one interpreter update.
/// The interpreter also returns submitted inputs, so these are not completion
/// notifications.
pub fn report_graph_events(events: &mut Vec<String>, mut report: impl FnMut(&str)) {
    for event in events.drain(..) {
        report(&event);
    }
}

#[cfg(test)]
mod tests {
    use super::SneakGraphState;

    #[test]
    fn r4_sneak_posture_enters_and_exits_once_per_change() {
        let mut state = SneakGraphState::default();
        assert_eq!(state.transition(false), None);
        assert_eq!(state.transition(true), Some("SneakStart"));
        for _ in 0..120 {
            assert_eq!(state.transition(true), None);
        }
        assert_eq!(state.transition(false), Some("SneakStop"));
        assert_eq!(state.transition(false), None);
        assert_eq!(state.transition(true), Some("SneakStart"));
    }
}
