# Preview3 backlog ID reconciliation

Rows are ordered by their first non-merge introduction in full Git history.
The oldest task keeps its ID. Six rows lost in prior merges are restored.
Identical stale copies keep separate rows with fresh IDs, as requested.
All 17 replacement IDs start above the previous maximum of their area.
Empty branch and dependency fields use the required dash spelling.

| Previous ID | Current ID | Task | First commit |
| --- | --- | --- | --- |
| HST-29 | HST-41 | Move placed NPCs in the walkthrough from their AI packages and animate them | `f9cf74243` |
| HST-30 | HST-42 | Keep NPCs on the ground: spawn them on cell collision and give them a character body | `f9cf74243` |
| HST-31 | HST-43 | Exclude third person head parts from the first person rig | `0414c0383` |
| HST-31 | HST-44 | Restore authored world collision at Whiterun and the Bannered Mare | `5a3396461` |
| HST-31 | HST-45 | Play authored door sequences at their child hinges with moving collision | `bd572c2ca` |
| HST-31 | HST-46 | Port Mudcrab cell lifecycle and demand priority with drawable handoff | `700edf3f3` |
| HST-31 | HST-47 | Carry and throw loose bodies through live player input | `682d714e7` |
| HST-31 | HST-48 | Prepare required load-door locations without blocking the host | `5d4ab8a63` |
| HST-32 | HST-49 | Spawn exterior persistent and temporary actors in the loaded grid | `5a3396461` |
| HST-32 | HST-50 | Measure the Mudcrab streaming port on the Bannered Mare gate return route | `700edf3f3` |
| HST-33 | HST-51 | Bound grab spring force so loose objects respond to their NIF mass | `922884892` |
| RND-40 | RND-47 | Restore exported NPC face color in the host material | `0414c0383` |
| RND-40 | RND-48 | Keep persistent Full LOD and never-fading models available to visible demand | `363092378` |
| TST-19 | TST-21 | Play path test: one scripted Whiterun route that checks what the player sees | `55636670b` |
| UI-31 | UI-37 | Expose the shared console in the player host | `a9ead5787` |
| UI-31 | UI-38 | Keep container hover observational and activation modal | `682d714e7` |
| UI-32 | UI-39 | Fit dialogue controls over the lower viewport | `682d714e7` |

Restored original IDs belong to Mudcrab grounded motion (HST-31), NPC pose
feedback (HST-32), market item activation (HST-33) and merchant barter (UI-31).
The recovered world collision and exterior actor rows are HST-44 and HST-49.
Original row sources are `adc381510`, `5a3396461` and `0d9c016f0`.

The console is now UI-37. Its serialized reference section remains named
`UI-31` so existing engine saves retain their meaning. That string is a save
format key. Barter tests retain their original UI-31 names. Current play-path
owners HST-29, HST-30 and TST-19 remain attached to the original active rows.

Historical whole-world residency measurements retain RND-40. Later bounded
visible-demand work uses RND-48. The face-color task is RND-47.

No feature is marked complete by this reconciliation. Existing statuses and
acceptance checks are retained. Unfinished recovered rows retain their
original in-progress status and no completion evidence.

## Round 4 id splits

Round 4 branches gave the same ID to different tasks, and the union merge of
BACKLOG.csv joined their titles into one row. Each task has its own row again.
The row that preview3 already had keeps its ID. Where preview3 had no row, the
first task in file order keeps the ID. Every other task takes the next free ID
of its area. Docs sections that named the old ID now name the new one.

Two names stay as they are because they are persisted or looked up by name.
The save sections named `INV-09` (flora harvest) and `CMB-40` (actor warning)
keep their strings so existing saves load. Test function names that start with
an old ID, such as `prf_50_` in `perks.rs` and `item_menu.rs`, are unchanged.

| Old ID | New ID | Task |
| --- | --- | --- |
| INV-09 | INV-12 | Harvest flora and trees through the shared world and restore their appearance |
| INV-09 | INV-13 | Harvest placed flora and ingredient trees through default activation |
| CMB-40 | CMB-51 | Honor actor warning radii before unprovoked combat acquisition |
| RND-47 | RND-62 | Cover the medium tree band with billboards beyond near cells |
| RND-47 | RND-63 | Preserve adult face skin color in the free walk renderer |
| HST-41 | HST-57 | Keep the animated first person rig on the camera heading |
| RND-48 | RND-64 | Sample native LAND textures instead of a coarse cell bake |
| PRF-48 | PRF-55 | Keep automatic gameplay time aligned with wall time during streaming |
| PRF-49 | PRF-56 | Warm shared tree and billboard pipelines before walking |
| HST-52 | HST-58 | Drive sneak graph state from shared actor posture |
| HST-52 | HST-59 | Give held objects mass-dependent sag and torque at the grab point |
| HST-52 | HST-60 | Restore activation through authored load doors |
| HST-52 | HST-61 | Resolve furniture entry failures found by activation sweep |
| HST-53 | HST-62 | Publish the correct view to furniture graph selectors |
| UI-40 | UI-43 | Load installed UI mods through the free walk launch settings |
| UI-41 | UI-44 | Honor configured icon movies in the live item menus |
| HST-54 | HST-63 | Place furniture users on authored markers and drive their entry and exit |
| UI-42 | UI-45 | Render cached vector fonts at native menu text sizes |
| HST-55 | HST-64 | Drive swimming from cell water and actor depth |
| RND-49 | RND-65 | Keep mountain object LOD while replacing source cells |
| RND-49 | RND-66 | Preserve diffuse detail when applying authored surface emission |
| RND-49 | RND-67 | Render the Whiterun streams with authored water materials |
| PRF-50 | PRF-57 | Keep cached item menu opening below the frame budget |
| PRF-50 | PRF-58 | Measure moving giant AI and animation costs in free walk |
| PRF-51 | PRF-59 | Bound item card shaping and large list construction in play |
| PRF-51 | PRF-60 | Reproduce offscreen moving giant frame spikes outside headless timing |
| RND-50 | RND-68 | Resolve the floating tree above the Whiterun plains |
| RND-51 | RND-69 | Render the player first person arms and equipped hands |
| RND-52 | RND-70 | Preserve SSE global skin indices for animated creatures |
| RND-53 | RND-71 | Apply the giant camp obelisk alternate texture set |
| RND-54 | RND-72 | Use the object clock for authored flame UV tracks |
| ANM-61 | ANM-64 | Keep NPC locomotion stable between physics ticks |
| ANM-61 | ANM-65 | Drive inline placed object behavior graphs and publish their poses |
| AI-96 | AI-100 | Keep free walk NPC routes clear of collision corners |
| ANM-62 | ANM-66 | Correct the giant forearm deformation shown in the free walk capture |
| ANM-62 | ANM-67 | Bridge object graph requests and events through threaded simulation |
| ANM-63 | ANM-68 | Deliver clip markers at the initial playhead once on activation |
| AI-97 | AI-101 | Advance grounded NPC routes past NAVM floor height differences |
