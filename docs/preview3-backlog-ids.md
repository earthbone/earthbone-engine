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
