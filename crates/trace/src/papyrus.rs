// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-19 health snapshots. Queue and registration meanings follow the local
//! CK OnUpdate and RegisterForSingleUpdate pages, timing follows performance.md.

use std::collections::{BTreeMap, VecDeque};

/// Live registrations, counted by timer mode rather than native call count.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct UpdateRegistrations {
    /// RegisterForUpdate timers.
    pub repeating: u64,
    /// RegisterForSingleUpdate timers.
    pub single: u64,
    /// RegisterForUpdateGameTime timers.
    pub repeating_game_time: u64,
    /// RegisterForSingleUpdateGameTime timers.
    pub single_game_time: u64,
}

/// Events awaiting their first instruction, and highest one-second arrival count.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventHealth {
    /// Handler deliveries awaiting execution.
    pub queued: u64,
    /// Maximum arrivals in any trailing second of the session.
    pub peak_per_second: u64,
}

/// One frame's health, with session peaks retained separately from live counts.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PapyrusHealth {
    /// Runnable stacks left when an instruction budget was exhausted.
    pub backlog: u64,
    /// Real seconds since the oldest deferred stack last received instructions.
    pub oldest_wait_seconds: f64,
    /// Active timers. Per-script rows include timers on that script's object.
    pub updates: UpdateRegistrations,
    /// Event names are normalized to lowercase.
    pub events: BTreeMap<String, EventHealth>,
    /// Sum of real seconds spent suspended during this frame across stacks.
    pub waiting_seconds: f64,
    /// CPU nanoseconds executing instructions during this frame.
    pub running_ns: u64,
    /// Live sleeping or native-latent stacks.
    pub suspended_stacks: u64,
}

impl PapyrusHealth {
    /// Add disjoint rows. Event peaks here are sums of the rows' individual peaks.
    pub fn add(&mut self, other: &Self) {
        self.backlog += other.backlog;
        self.oldest_wait_seconds = self.oldest_wait_seconds.max(other.oldest_wait_seconds);
        self.updates.repeating += other.updates.repeating;
        self.updates.single += other.updates.single;
        self.updates.repeating_game_time += other.updates.repeating_game_time;
        self.updates.single_game_time += other.updates.single_game_time;
        self.waiting_seconds += other.waiting_seconds;
        self.running_ns = self.running_ns.saturating_add(other.running_ns);
        self.suspended_stacks += other.suspended_stacks;
        for (name, value) in &other.events {
            let event = self.events.entry(name.clone()).or_default();
            event.queued += value.queued;
            event.peak_per_second += value.peak_per_second;
        }
    }
}

/// Arrival meter retaining only the trailing second, batching identical timestamps.
#[derive(Debug, Default)]
pub struct EventRate {
    arrivals: VecDeque<(f64, u64)>,
    count: u64,
    peak: u64,
}

impl EventRate {
    /// Record arrivals at a monotonically increasing real-time timestamp.
    pub fn record(&mut self, seconds: f64, count: u64) {
        while self
            .arrivals
            .front()
            .is_some_and(|(time, _)| seconds >= time + 1.0)
        {
            if let Some((_, expired)) = self.arrivals.pop_front() {
                self.count -= expired;
            }
        }
        if let Some((time, previous)) = self
            .arrivals
            .back_mut()
            .filter(|(time, _)| *time == seconds)
        {
            *time = seconds;
            *previous += count;
        } else {
            self.arrivals.push_back((seconds, count));
        }
        self.count += count;
        self.peak = self.peak.max(self.count);
    }

    /// Highest one-second count observed, including the current partial second.
    #[must_use]
    pub const fn peak(&self) -> u64 {
        self.peak
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_19_event_peak_uses_a_trailing_second_and_retains_the_session_peak() {
        let mut rate = EventRate::default();
        rate.record(0.9, 250);
        rate.record(1.1, 250);
        assert_eq!(rate.peak(), 500);
        rate.record(1.9, 250);
        assert_eq!(rate.peak(), 500);
        rate.record(3.0, 1);
        assert_eq!(rate.peak(), 500);
        assert_eq!(rate.arrivals.len(), 1);
    }
}
