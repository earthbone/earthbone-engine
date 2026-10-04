# soe-bench: performance harness

Commands, scenarios and timing budgets are documented in
[performance.md](performance.md) and [performance-limits.md](performance-limits.md).

`soe-bench frame-budget` runs a fixed PRF-16 workload with 1,000 travel and
sandbox actors, a record-backed NAVM, ten warm-up ticks and 3,600 measured
ticks paced over 60 seconds. It verifies movement, navigation and package
selection for every actor. It emits one `SOE_BUDGET` JSON line containing
full-run CPU frame and PRF-12 per-system p50, p99 and worst milliseconds,
including idle slots. The suite runner applies the budgets in performance.md.
Setup, pacing and reporting are outside the tick timing window. The command
rejects arguments that could change the fixed workload.

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
