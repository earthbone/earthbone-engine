# soe-bench: performance harness

Commands, scenarios and timing budgets are documented in
[performance.md](performance.md) and [performance-limits.md](performance-limits.md).

The stress runner captures a `soe_trace::WorkFrame` around each measured
step and prints the maximum count per operation for each sample window as
`work_peak_<operation> <count> count`. Setup, warm-up and reports are excluded.
Nested Game ticks enter the same window. Fixed `prf_14_` tests run the existing
PRF-07 world with 1,000 travel and sandbox actors, ten warm-up ticks and
120 measured ticks, plus the mixed
fixture with 16 actors, scripts and paths. They check every operation against
fixed limits without elapsed-time assertions and verify the fixture's actor
movement and script completion. The scenarios follow local
`godotwind/docs/systems/benchmarking.md` guidance on fixed workload and
separate setup.

Allocation samples read `soe-trace::MemorySnapshot` from the host counting
allocator, enabled by the benchmark's `profile-memory` dependency feature.
They report successful allocation and reallocation request counts, and the
sum of requested sizes, between sample boundaries. This uses the same Rust
allocation accounting as the host, with one process allocator. Reallocation
bytes describe the new request's size. Godot's own heap is outside the sample.
