// SPDX-License-Identifier: GPL-3.0-or-later

//! Scoped allocation request counts for synthetic benchmark samples.

use std::alloc::System;

use stats_alloc::{Region, Stats, StatsAlloc, INSTRUMENTED_SYSTEM};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

pub struct Sample(Region<'static, System>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peak {
    pub calls: usize,
    pub bytes: usize,
}

pub fn begin_sample() -> Sample {
    Sample(Region::new(GLOBAL))
}

pub fn end_sample(sample: &Sample) -> Peak {
    let stats: Stats = sample.0.change();
    Peak {
        calls: stats.allocations + stats.reallocations,
        bytes: stats
            .bytes_allocated
            .saturating_add(stats.bytes_reallocated.max(0) as usize),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_06_counts_heap_requests_in_the_sample_region() {
        let sample = begin_sample();
        let payload = vec![0_u8; 4096];
        let peak = end_sample(&sample);
        assert!(peak.calls >= 1, "one heap request must be counted");
        assert!(
            peak.bytes >= payload.len(),
            "requested bytes must be counted"
        );
        std::hint::black_box(payload);
    }
}
