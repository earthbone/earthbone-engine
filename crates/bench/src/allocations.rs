// SPDX-License-Identifier: GPL-3.0-or-later

//! Scoped allocation request counts for synthetic benchmark samples.

use soe_trace::MemorySnapshot;

pub struct Sample(MemorySnapshot);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peak {
    pub calls: usize,
    pub bytes: usize,
}

pub fn begin_sample() -> Sample {
    Sample(MemorySnapshot::now())
}

pub fn end_sample(sample: &Sample) -> Peak {
    let end = MemorySnapshot::now();
    Peak {
        calls: end
            .allocation_calls
            .iter()
            .zip(sample.0.allocation_calls)
            .map(|(end, start)| end.saturating_sub(start) as usize)
            .sum(),
        bytes: end
            .allocated_bytes
            .iter()
            .zip(sample.0.allocated_bytes)
            .map(|(end, start)| end.saturating_sub(start) as usize)
            .sum(),
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
