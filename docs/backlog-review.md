# Backlog review and model recommendations

Reviewed against master 6d483cc on 2026-09-20. The original 201-row CSV was
copied into the isolated backlog-review branch because another writer has
uncommitted edits in the master checkout. No original row ID was renumbered
or removed. This review changes planning data, not engine behavior.

## Recommendation columns

The CSV adds difficulty, recommended_model and recommended_effort after status,
then breaker_group and judge_group.
Difficulty describes implementation uncertainty and integration risk, not
elapsed runtime or priority. Recommendations assume the row's dependencies
are complete. They are engineering estimates, not measured model benchmarks
or a guarantee that the row fits one context window.

| Difficulty | Meaning | Usual model and effort |
|---|---|---|
| low | A bounded mechanical change or execution of an established check | GPT-5.6-Luna at medium, or Sol for operational verification |
| medium | A known format or focused implementation with clear acceptance | GPT-5.6-Terra at medium |
| high | Stateful behavior, compatibility reasoning or several interacting paths | GPT-6-Astra at high |
| very_high | Undocumented formats, deep execution state or deep format uncertainty | GPT-6-Astra at xhigh |

Operational corpus, benchmark and installation checks generally use
GPT-5.6-Sol. Physical speaker verification and long soaks still need their
hardware or elapsed time. A model recommendation does not supply either.
Completed rows retain estimates for comparable future work. SCR-06 is dropped
as duplicate scope and must not be scheduled despite retaining recommendation
metadata. GPT-5.5 is available but is not the preferred default for these rows.

Model IDs and effort combinations were checked against the local Codex
models_cache.json catalog. Only entries visible in the model picker were
considered. The catalog lists gpt-6-astra, gpt-5.6-sol, gpt-5.6-terra,
gpt-5.6-luna and gpt-5.5. The recommendations use medium, high and xhigh,
all supported by their selected models. They do not change the active model
or authorize automatic delegation.

The [official Codex model command documentation](https://learn.chatgpt.com/docs/developer-commands#built-in-slash-commands)
confirms that /model selects a model and its available reasoning effort.
The per-task choices in this file are project recommendations, not assignments
prescribed by OpenAI. No pricing or account-capacity assumption was used.

### Measured selection, 2026-09-27

A comparison of Codex models on DLG-03, PRG-09, FMT-18, MAG-07 and MAG-39
replaced the estimates above for every open row. Each open row now recommends
gpt-6-luna at high effort for the writer, the breaker and the judge. Done rows
keep their earlier estimates.

- gpt-6-luna high met the FMT-18 check alone. On MAG-07 it missed the
  ingestible clause. On MAG-39, a very_high row, its writer missed the
  Summonable rule, its breaker found the gap and its judge fixed it. All three
  steps cost $0.14 at API prices.
- gpt-6-sol high met every check it was given, the Summonable rule included,
  without a breaker. Its MAG-39 writer alone cost $1.56. It is the rerun model
  when a luna judge reports a row as partial.
- gpt-6-luna xhigh stalled on MAG-39 without an edit. gpt-6-luna low has not
  completed a reviewed row.
- gpt-5.6-terra high matched luna on MAG-07 at about nine times the price.

### Checker groups

breaker_group and judge_group name the rows one breaker and one judge cover,
so each checker reads the shared code once and reuses its cache across rows.
`tools/backlog_groups.py BACKLOG.csv --write` recomputes both columns and the
model columns of open rows after rows change.

- Rows share a breaker group when they share an id prefix and the first source
  file read_first names, and sit at the same dependency depth among open rows,
  so their writers can run in parallel.
- A row alone in its topic joins the lone rows of its area and crate, then of
  its area, then of any area, at the same depth.
- A breaker group holds up to 8 weight and a judge group up to 4, where a
  very_high row weighs 2 and any other row 1. The caps keep a checker under the
  275k context limit of the GPT models. On 2026-09-27 a single very_high judge
  peaked at 127k and its breaker at 55k. The first grouped runs confirm or
  lower the caps.
- Names read `B-<area>-<topic>-<n>` for breakers and `J-<area>-<topic>-<n>` for
  judges, with a letter suffix when one breaker group splits into several
  judge groups.

## Confirmed corrections applied

| Rows | Finding and correction | Evidence |
|---|---|---|
| CMB-01, CMB-02 | Replace the vague gate-not-rerun evidence with the recorded merged validation | e9cc2f9, combat_hits and combat_lifecycle tests, prior gate logs |
| AI-11 | Mark the Detour-style funnel implementation done | 8d7641c, crates/navmesh/src/graph/funnel.rs, graph/funnel_tests.rs and installed route survey |
| SAV-05 | Mark the decodable inventory restoration scope done and remove its unrelated query dependency | e9cc2f9, world/inventory_restore.rs, vanilla_save/inventory.rs |
| AUD-04 | Narrow to SOUN support instead of asking to rebuild existing SNDR/SNCT/SOPM handling | crates/game/src/sound.rs and tests/sound.rs |
| FMT-03 | Narrow to PACK/PERK/SCEN fragment tails and correct its crate to game | crates/game/src/vmad.rs already parses QUST and INFO, with the other tails retained raw |
| FMT-04 | Separate DLBR/SCEN record fields from the fragment work | FMT-03 owns remaining VMAD tails, so a second fragment task would overlap |
| PRG-05 | Narrow to item and spell lists, acknowledging existing actor-template selection | world/templates.rs handles LVLN level filtering and chance-none |
| SAV-01, SAV-11 and their split rows | Enumerate remaining change-form families without relisting merged restoration as absent | world/reference_restore.rs, world/npc_restore.rs and crates/save/src/npc.rs |
| SCR-01 and its split rows | Inventory and divide all 179 current stub registrations | stubs registration arrays contain 26 bool, 8 int, 79 method, 4 float and 62 global entries |
| SCR-06 | Drop duplicate stack/latent restoration scope in favor of SAV-02 and SAV-03 | Original row titles and acceptance checks overlap |
| INV-02 | Define reference/FormList/keyword queries and preserve base-form aggregation semantics | Local CK GetItemCount_-_ObjectReference.wikitext |
| SAV-14 | Add the completed empty PackageData milestone without claiming named-state support | 6d483cc and docs/save-package-data.md |

The resulting CSV contains 538 rows: five done, 532 open and one dropped.
This is a task inventory, not an engine completion percentage. The increase
comes from dividing bundled requirements. Every original ID remains present.
A narrowed original ID owns the first bounded slice of its former scope, and
the table below identifies the other slices.

## Scope corrections

The former whole-system rows now enumerate their contracts:

- 373 condition functions outside the current 29-function allowlist, divided
  into 50 batches of at most eight named functions. The xEdit inventory includes
  legacy and extension functions. Their presence does not prove vanilla SE
  uses them, and a missing reference contract requires explicit classification.
- 18 named package procedures, each with its own execution and lifecycle check.
- 42 named magic archetypes beyond the existing Value, Script and Dual Value
  handlers, plus one research row for unnamed values 13 and 14.
- Ten identified behavior node execution contracts and a separate installed
  graph inventory for classes not yet modeled.
- All 179 quiet native registrations, divided into 48 binding tasks with at
  most five bindings. Method and global registrations remain distinct.
  There are also 20 audit tasks covering 40 inventoried native classes.
- All 100 shim types in the design's planned surface, divided into 21 batches.
  These remain constrained to the MIT API boundary.
- 45 change-form type tags beyond REFR, ACHR, QUST and NPC_, in 16 families.
  Each family has separate decoding and runtime application tasks. Existing
  restoration does not imply that all fields of the four excluded tags work.
  Their known remaining fields stay in the reference, actor and quest rows.

Combat, services, crafting, progression, animation tracks, visual effects,
editor operations and other bundled features were also divided. MCM lifecycle
and its option families are separate, as are landscape heights, textures and
colors. Each new row carries its own acceptance contract and model estimate.

Dependencies were corrected after splitting. Arrow recovery depends on flight,
combat perk application depends on the entry-point dispatcher, dialogue mouth
motion depends on viseme mapping, and skin distribution depends on skin
assembly. Save integration names its restoration passes instead of depending
only on the first narrowed change-form family.

Additional corrections:

- HST-07 follows authored door links to interior destinations. It does not
  compare positions belonging to unrelated cell coordinate systems.
- HST-05 requires a human observation on the physical output. A loopback
  recording alone cannot establish that a speaker was audible.
- HST-06 requires its performance budget before the corpus measurement.
- CRF-04 uses existing stack metadata and persistence, with selection and
  mutation of crafted instances as its remaining scope.
- RND-09 acknowledges existing FURN and DOOR filtering and requires an
  installed-record survey with drawn counts and false-positive checks.
- EDT-01 names the planned editor crate and esm. The editor crate is an
  explicit future crate from docs/editor.md, not an existing directory.

## How to use the rows

A done_check prefixed with Planned check names future acceptance work. It is
not a claim that a test of that name already exists. An implementation must
supply the executable test, survey or recorded manual check and put its actual
command, result and commit in evidence when marking the row done.

The single-context estimate assumes completed prerequisites and a fixed
contract. No static audit can guarantee a token count for undocumented data.
Before claiming a row, check its named sources and current implementation.
If the contract exceeds one 300k context, split it with stable new IDs before
starting. Keep every discovered remainder in a named row and update dependent
rows. Increasing reasoning effort is not a substitute for reducing scope.

Native binding rows own API argument handling and routing into their owning
system. They must not silently absorb missing gameplay systems. Add the
specific system prerequisite before claiming such a row. Research and corpus
classification rows finish with an inventory and explicitly scoped follow-up
rows where implementation is needed. Recording an unsupported case does not
complete the corresponding gameplay feature.

SE playability remains the current priority. LE packfiles, newer PEX layouts,
Switch audio, editor work and broad binary shim coverage are retained in the
inventory without making them prerequisites for ordinary SE play.

## Validation and limits

The audit checks every row for stable IDs, required fields, model and effort
availability, title length, two-crate and five-source limits, CSV quoting and
valid dependencies. The dependency graph has no cycles. Every original ID is
preserved. The enumerated condition, native, shim and change-form inventories
have no duplicated members within their respective scopes.

Code inspection confirmed the corrections above and the named source
inventories. This planning audit is not an independent end-to-end test of
every engine feature or proof that no undiscovered compatibility gaps exist.
No gameplay code changed. No original-runtime comparison, Godot visual check
or physical speaker check was performed in this audit.

Known failures remain visible: the strict save corpus still reports 81 named
PackageData states, and the integrated actor inventory survey excludes 53
records after restoring 114,769 inventories. These are prior engine results.
The completed SAV-05 and SAV-14 rows do not claim full ESS restoration.

The working changes are isolated on backlog-review. The original master
BACKLOG.csv and AGENTS.md remained byte-identical to the source snapshots.
No commit or merge has been made for this review.

## Split scope map

Each entry lists the retained ID followed by the IDs carrying its separated
requirements. Use this map when reviewing an old reference to a broad task.

| Original row | Resulting rows |
|---|---|
| CMB-05 | CMB-05, CMB-20 |
| CMB-06 | CMB-06, CMB-21 |
| CMB-08 | CMB-08, CMB-22, CMB-23 |
| CMB-10 | CMB-10, CMB-24, CMB-25 |
| CMB-15 | CMB-15, CMB-26 |
| CMB-19 | CMB-19, CMB-27 |
| MAG-02 | MAG-02, MAG-16, MAG-17 |
| MAG-03 | MAG-03, MAG-18 |
| MAG-06 | MAG-06, MAG-19 |
| MAG-07 | MAG-07, MAG-20, MAG-21 |
| MAG-09 | MAG-09, MAG-22 |
| MAG-12 | MAG-12, MAG-23 |
| MAG-14 | MAG-14, MAG-24, MAG-25 |
| MAG-15 | MAG-15, MAG-26 |
| DLG-02 | DLG-02, DLG-14 |
| DLG-03 | DLG-03, DLG-15 |
| DLG-08 | DLG-08, DLG-16, DLG-17 |
| DLG-09 | DLG-09, DLG-18, DLG-19 |
| DLG-11 | DLG-11, DLG-20 |
| DLG-13 | DLG-13, DLG-21 |
| CRF-03 | CRF-03, CRF-07 |
| CRF-05 | CRF-05, CRF-08 |
| PRG-01 | PRG-01, PRG-07 |
| PRG-02 | PRG-02, PRG-08 |
| PRG-03 | PRG-03, PRG-09 |
| PRG-04 | PRG-04, PRG-10 |
| PRG-05 | PRG-05, PRG-11 |
| PRG-06 | PRG-06, PRG-12, PRG-13 |
| INV-01 | INV-01, INV-05, INV-06 |
| INV-03 | INV-03, INV-07, INV-08 |
| AI-02 | AI-02, AI-13, AI-14, AI-15 |
| AI-04 | AI-04, AI-16 |
| AI-09 | AI-09, AI-17 |
| AI-12 | AI-12, AI-18, AI-19 |
| ANM-01 | ANM-01, ANM-11 |
| ANM-05 | ANM-05, ANM-12 |
| ANM-10 | ANM-10, ANM-13 |
| HST-04 | HST-04, HST-10 |
| HST-09 | HST-09, HST-11 |
| RND-03 | RND-03, RND-10 |
| RND-04 | RND-04, RND-11 |
| RND-06 | RND-06, RND-12 |
| RND-08 | RND-08, RND-13, RND-14 |
| AUD-05 | AUD-05, AUD-09 |
| AUD-06 | AUD-06, AUD-10 |
| SAV-06 | SAV-06, SAV-15, SAV-16 |
| SAV-07 | SAV-07, SAV-17 |
| SAV-08 | SAV-08, SAV-18 |
| SAV-09 | SAV-09, SAV-19 |
| SAV-10 | SAV-10, SAV-20 |
| SAV-12 | SAV-12, SAV-21, SAV-22 |
| FMT-01 | FMT-01, FMT-16 |
| FMT-03 | FMT-03, FMT-17, FMT-18 |
| FMT-08 | FMT-08, FMT-19 |
| FMT-09 | FMT-09, FMT-20, FMT-21, FMT-22, FMT-23 |
| FMT-13 | FMT-13, FMT-24 |
| FMT-14 | FMT-14, FMT-25 |
| FMT-15 | FMT-15, FMT-26 |
| MOD-06 | MOD-06, MOD-08, MOD-09, MOD-10 |
| UI-04 | UI-04, UI-10 |
| EDT-02 | EDT-02, EDT-13 |
| EDT-03 | EDT-03, EDT-14, EDT-15 |
| EDT-04 | EDT-04, EDT-16, EDT-17 |
| EDT-07 | EDT-07, EDT-18, EDT-19, EDT-20 |
| EDT-09 | EDT-09, EDT-21 |
| EDT-10 | EDT-10, EDT-22, EDT-23 |
| EDT-11 | EDT-11, EDT-24 |
| EDT-12 | EDT-12, EDT-25 |
| MAG-05 | MAG-05, MAG-27, MAG-28, MAG-29, MAG-30, MAG-31, MAG-32, MAG-33, MAG-34, MAG-35, MAG-36, MAG-37, MAG-38, MAG-39, MAG-40, MAG-41, MAG-42, MAG-43, MAG-44, MAG-45, MAG-46, MAG-47, MAG-48, MAG-49, MAG-50, MAG-51, MAG-52, MAG-53, MAG-54, MAG-55, MAG-56, MAG-57, MAG-58, MAG-59, MAG-60, MAG-61, MAG-62, MAG-63, MAG-64, MAG-65, MAG-66, MAG-67, MAG-68 |
| AI-03 | AI-03, AI-20, AI-21, AI-22, AI-23, AI-24, AI-25, AI-26, AI-27, AI-28, AI-29, AI-30, AI-31, AI-32, AI-33, AI-34, AI-35, AI-36 |
| ANM-03 | ANM-03, ANM-14, ANM-15, ANM-16, ANM-17, ANM-18, ANM-19, ANM-20, ANM-21, ANM-22, ANM-23 |
| AI-01 | AI-01, AI-37, AI-38, AI-39, AI-40, AI-41, AI-42, AI-43, AI-44, AI-45, AI-46, AI-47, AI-48, AI-49, AI-50, AI-51, AI-52, AI-53, AI-54, AI-55, AI-56, AI-57, AI-58, AI-59, AI-60, AI-61, AI-62, AI-63, AI-64, AI-65, AI-66, AI-67, AI-68, AI-69, AI-70, AI-71, AI-72, AI-73, AI-74, AI-75, AI-76, AI-77, AI-78, AI-79, AI-80, AI-81, AI-82, AI-83, AI-84, AI-85 |
| SCR-01 | SCR-01, SCR-08, SCR-09, SCR-10, SCR-11, SCR-12, SCR-13, SCR-14, SCR-15, SCR-16, SCR-17, SCR-18, SCR-19, SCR-20, SCR-21, SCR-22, SCR-23, SCR-24, SCR-25, SCR-26, SCR-27, SCR-28, SCR-29, SCR-30, SCR-31, SCR-32, SCR-33, SCR-34, SCR-35, SCR-36, SCR-37, SCR-38, SCR-39, SCR-40, SCR-41, SCR-42, SCR-43, SCR-44, SCR-45, SCR-46, SCR-47, SCR-48, SCR-49, SCR-50, SCR-51, SCR-52, SCR-53, SCR-54 |
| SCR-02 | SCR-02, SCR-55, SCR-56, SCR-57, SCR-58, SCR-59, SCR-60, SCR-61, SCR-62, SCR-63, SCR-64, SCR-65, SCR-66, SCR-67, SCR-68, SCR-69, SCR-70, SCR-71, SCR-72, SCR-73 |
| SHM-03 | SHM-03, SHM-04, SHM-05, SHM-06, SHM-07, SHM-08, SHM-09, SHM-10, SHM-11, SHM-12, SHM-13, SHM-14, SHM-15, SHM-16, SHM-17, SHM-18, SHM-19, SHM-20, SHM-21, SHM-22, SHM-23 |
| SAV-11 | SAV-11, SAV-23, SAV-25, SAV-27, SAV-29, SAV-31, SAV-33, SAV-35, SAV-37, SAV-39, SAV-41, SAV-43, SAV-45, SAV-47, SAV-49, SAV-51 |
| SAV-01 | SAV-01, SAV-24, SAV-26, SAV-28, SAV-30, SAV-32, SAV-34, SAV-36, SAV-38, SAV-40, SAV-42, SAV-44, SAV-46, SAV-48, SAV-50, SAV-52 |
| EDT-05 | EDT-05, EDT-26, EDT-27 |
| EDT-06 | EDT-06, EDT-28 |
| UI-05 | UI-05, UI-11, UI-12, UI-13 |
| UI-07 | UI-07, UI-14 |
