# Runtime threading, PRF-17

## Original runtime evidence

The local CommonLibSSE-NG headers in `engine-refs` distinguish Papyrus dispatch
from bytecode execution. `include/RE/V/VirtualMachine.h` declares `Update` and
`UpdateTasklets` with separate budgets, function and suspend queues, running
stacks and attached-script locks. `include/RE/C/CodeTasklet.h` associates a
tasklet with its stack and records function-call, return and retry resume reasons.
`include/RE/S/Stack.h` distinguishes latent waits from waits for another stack's
call or return. `include/RE/O/Object.h` contains a `lockStructure` for each script
object, distinct from its reference-count and handle lock.

The CK [Papyrus INI settings][ini] describe `fUpdateBudgetMS` as the main
dispatch-loop allowance, with a default of 1.2 milliseconds.
`fExtraTaskletBudgetMS`, also 1.2 milliseconds by default, gives bytecode tasklets
extra time on another game thread when their normal shared thread is busy.
It is not an instruction count or a multiplier for each worker.

Mods depend on shared script-variable and array identity, call and return
ordering, and resuming latent calls with their result. The shipped
`skse64/scripts/vanilla/Utility.psc` distinguishes gameplay, menu and game-time
waits. A waiting call cannot hold up every other script. Per-object locking
must account for external calls and returning stacks, rather than just locking
each opcode independently. The requested CK [Threading Notes][threading] page
is absent from the local `ck-wiki` dump and returned HTTP 403 during this work.
The headers establish lock storage but do not establish the complete lock
handoff rules. Those rules have not been verified here.

AI is divided into high, middle-high, middle-low and low process levels in
`include/RE/A/AIProcess.h`, with a current package, targets and per-actor cached
values. `include/RE/P/ProcessLists.h` has a movement synchronization semaphore,
package-location initialization queue and lock, and position synchronization
queue. These establish synchronization boundaries, not the exact number of
vanilla AI workers or which package operations run on each thread. Package
selection, condition results, target identity, movement and emitted events are
observable by mods. The engine's worker completion order must not choose them.

## Engine navigation workers

Pending per-actor navigation searches run on Rayon workers against a borrowed,
read-only NAVM graph and mesh-to-cell table. Each job owns one mutable request.
The dispatcher reserves up to 64 nodes per request and at most 512 nodes per
frame before dispatch. Unused reservations are returned before dispatching the
next batch. Results are applied in the existing rotating FormID order, retaining
the same budget and starvation rules with one or several workers. Graph
revisions invalidate pending requests before dispatch. No world state is copied
into a second authoritative store.

The existing query algorithm follows local
`recastnavigation/Detour/Source/DetourNavMeshQuery.cpp`,
`updateSlicedFindPath(maxIter, doneIters)`. Rayon supplies scheduling rather than
a new search implementation. Its [parallel iterator and pool APIs][rayon] allow
disjoint request borrows with a shared immutable graph. No unsafe code or
per-actor host nodes are introduced.

Initial request setup and its first slice, package evaluation, conditions,
marker selection, movement preparation and combat remain on the calling thread. Papyrus
also remains on that thread, with its existing instruction budget and shared
`Rc<RefCell<_>>` values and native closures. This implementation does not satisfy
PRF-17's full AI and Papyrus worker execution or vanilla per-object lock contract.

The `prf_17_` navigation tests compare every frame with the previous serial
dispatcher for mixed route lengths and flee searches, including node refunds,
request progress, results and PRF-14 work counts. They check cancellation,
graph revisions, idle metrics and PRF-12 attribution of dispatch/join time.
Integration checks compare world and VM save bytes with one and eight workers,
using PRF-12's 1,000-actor fixture and the installed Bannered Mare cell.
The stress fixture's short routes finish in the caller's first slice, so it
launches no resumed navigation jobs and does not establish worker scaling.
The mixed long-route and flee test measures navigation jobs separately. Its
worker busy and caller wait times do not measure Papyrus or application locks.

PRF-36 adds waypoint steering to the same pool, borrowing a detached AiState.
The caller joins each job before applying movement and arrival effects in
FormID order. These jobs share the configured pool with navigation. Separate
movement job, worker busy and caller wait metrics include their dispatch costs.
Without a configured pool, steering runs on the caller and reports no worker
jobs. This avoids a global-pool dispatch and join for
each actor's short waypoint step.
Distance-based process scheduling and immutable PACK and procedure-tree caches
reduce caller work. [game.md](game.md) specifies the cadence policy. World
mutation, package conditions, RNG and event emission remain on the caller.

[ini]: https://ck.uesp.net/w/index.php?title=INI_Settings_%28Papyrus%29&veaction=edit
[threading]: https://ck.uesp.net/wiki/Threading_Notes_(Papyrus)
[rayon]: https://docs.rs/rayon
