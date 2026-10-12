# PRF-17 writer reply

The starting branch already runs resumed navigation searches and waypoint
steering through a configurable Rayon pool. Results are applied in actor order,
and worker busy time and caller dispatch/join time are recorded. Existing
`prf_17_` tests cover deterministic one-worker and eight-worker runs and idle
worker metrics. The PRF-12 stress test records AI and Papyrus timings but does
not assert worker speedup. Its short routes complete in the caller's first
slice, so it does not exercise resumed navigation workers.

I read the CK Papyrus INI settings page. It gives separate 1.2 millisecond
defaults for `fUpdateBudgetMS` and `fExtraTaskletBudgetMS`, with the latter
allowing tasklet work on another game thread. The local `ck-wiki` corpus has no
Papyrus threading notes page. CommonLibSSE-NG `VirtualMachine.h` has separate
dispatch and tasklet update methods and queues. `Object.h` has a per-object
lock structure. `Stack.h` distinguishes latent waits from waits on another
stack, and `CodeTasklet.h` records resume reasons. `AIProcess.h` names four
process levels. `ProcessLists.h` exposes movement and package-location
synchronization structures. These declarations do not specify the full lock
handoff contract.

The remaining clauses need a wider implementation. Package selection still
reads and updates `World` on the caller. Papyrus execution still runs on the
caller and owns `Rc<RefCell<_>>` state plus native closures that access the
shared world. Rayon jobs require sendable work and synchronized shared
closures. No per-object Papyrus lock or lock-wait telemetry exists. The current
code does not provide a safe worker boundary for these operations, so I have
not added a worker count or a speedup assertion that the implementation cannot
meet.

Checks were not run. Light mode disallows the existing PRF-12 stress test, and
no implementation or test code changed in this attempt. The PRF-17 row remains
partial pending a sendable Papyrus worker model, per-object lock semantics,
parallel package evaluation over a read-only view, and a stress test that
asserts measured worker scaling.
