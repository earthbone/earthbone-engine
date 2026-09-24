export const meta = {
  name: 'soe-backlog',
  description: 'Implement prepared BACKLOG.csv clusters: one writer per cluster, a breaker, an opus judge that fixes and gates, and a serial merge queue',
  whenToUse: 'After tools/backlog_batch.py prepare created one worktree per cluster. Pass its JSON as args. Runs args.concurrency lanes, each with its own reusable build directory.',
  phases: [
    { title: 'Write', detail: 'one opus writer explores the subsystem once and implements the cluster rows in turn, committing each', model: 'opus' },
    { title: 'Break', detail: 'breaker writes adversarial tests across the cluster' },
    { title: 'Judge', detail: 'opus fixes what the tests expose, writes the planned checks, runs the gate, rules per row', model: 'opus' },
    { title: 'Merge', detail: 'one cluster at a time: merge master in, gate, merge, clean up' },
  ],
}

const SKILLS = '/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/.claude/skills'
const BREAKER_DEF = '/var/home/pengolodh/.claude/agents/breaker.md'
const GODOT = '/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/tools/godot/Godot_v4.7.2-stable_linux.x86_64'
const GODOT_FLOOR = '/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/tools/godot/Godot_v4.4.1-stable_linux.x86_64'

const PROFILES = {
  native: {
    skills: ['soe-native', 'soe-corpus'],
    write: 'Semantics come from ck-wiki pages and play behaviour, never from the binary. Register the native through the crate macros, remove it from the stub table and update the conformance table the soe-native skill names.',
    verify: 'Write one conformance fixture per native covering the CK wiki contract, including the None and invalid-receiver results the wiki defines.',
    break: 'None receivers, wrong argument types, forms of the wrong type, latent and reentrant calls, state that must survive engine save and load, failures that must leave no partial effect, and DLC forms.',
  },
  gameplay: {
    skills: ['soe-corpus', 'soe-record-view', 'soe-live-run'],
    write: 'Read forms from the winning override through the shared World, never engine-private copies. When saved state changes, bump the SOEW version and keep reading the previous one. Reject invalid input atomically.',
    verify: 'Write the planned check as a synthetic-plugin integration test in crates/game/tests, following the fixtures already there. Add a SKYRIM_DATA gated case when the check names vanilla records.',
    break: 'Load-order overrides, missing or malformed records, engine save and load continuation, older saves without the new state, partial failure atomicity, event ordering, and Dawnguard, HearthFires and Dragonborn forms.',
  },
  format: {
    skills: ['soe-corpus', 'soe-record-view', 'soe-live-run'],
    write: 'Grep ../engine-refs for the format, class and tool names before the first line of a parser, follow what is declared there and credit it in the crate doc. If you write your own, say in the doc what you read and why it did not fit.',
    verify: 'Write the planned check against synthetic bytes written by the test. Add a SKYRIM_DATA gated sweep over installed files when the check names real data, and print its counts.',
    break: 'Truncated input, wrong sizes and counts, decompression size mismatch, integer overflow, deep nesting, round trips, and duplicate identifiers. Extend the matching target under fuzz/ when one exists.',
  },
  host: {
    skills: ['soe-godot-host', 'soe-corpus'],
    write: 'Stock Godot only, at the 4.4 API floor, tested on 4.7.2. No Godot patches. Keep game state in the shared Game and World, the host only presents it.',
    verify: 'Cover the Rust data path with a test, then write or extend a godot/*_test.gd acceptance script that prints one RESULT: PASS or RESULT: FAIL line and exits non-zero on failure. Declare budgets before running.',
    break: 'Rust-side data path failures, rejected or missing resources, stale state after a data source change, repeated open and close cycles, and budgets that are only met on a warm cache.',
    godot: true,
  },
  shim: {
    skills: ['soe-corpus'],
    write: 'commonlib-shim is MIT. No code, comment or layout may come from a GPL source. Take declarations from CommonLibSSE-NG or another MIT-compatible source and name it in docs/shim.md.',
    verify: 'Write the C++ contract fixture the planned check names and compile it with clang++ -fsyntax-only.',
    break: 'Size, offset and alignment assertions against the declared layout, enum values, null handles, and anything the fixture accepts that the declared contract rejects.',
  },
  tooling: {
    skills: ['soe-perf', 'soe-live-run'],
    write: 'Keep tools small, report the numbers they measure and never claim a result that was not measured.',
    verify: 'Write the planned check so it runs in the tier 1 gate unless it needs installed data.',
    break: 'False passes: the tool must fail on broken input, empty input and a missing dependency, and its exit status must match its printed result.',
  },
}

const TESTS = {
  type: 'array',
  items: {
    type: 'object',
    properties: { file: { type: 'string' }, command: { type: 'string' } },
    required: ['file', 'command'],
  },
}
const FAILURES = {
  type: 'array',
  items: {
    type: 'object',
    properties: { test: { type: 'string' }, failure: { type: 'string' } },
    required: ['test', 'failure'],
  },
}
const WRITE = {
  type: 'object',
  properties: {
    build_ok: { type: 'boolean' },
    rows: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          id: { type: 'string' },
          implemented: { type: 'boolean' },
          blocker: { type: 'string' },
          commit: { type: 'string' },
          summary: { type: 'string' },
          files_changed: { type: 'array', items: { type: 'string' } },
          known_gaps: { type: 'array', items: { type: 'string' } },
        },
        required: ['id', 'implemented', 'blocker', 'commit', 'summary', 'files_changed', 'known_gaps'],
      },
    },
  },
  required: ['build_ok', 'rows'],
}
const BREAK = {
  type: 'object',
  properties: {
    tests: TESTS,
    bugs: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          row: { type: 'string' },
          test: { type: 'string' },
          severity: { type: 'string', enum: ['high', 'medium', 'low'] },
          expected: { type: 'string' },
          observed: { type: 'string' },
          source: { type: 'string' },
        },
        required: ['row', 'test', 'severity', 'expected', 'observed', 'source'],
      },
    },
  },
  required: ['tests', 'bugs'],
}
const JUDGE = {
  type: 'object',
  properties: {
    analysis: { type: 'string' },
    rows: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          id: { type: 'string' },
          verdict: { type: 'string', enum: ['done', 'partial', 'failed', 'skipped'] },
          check_tests: TESTS,
          open_issues: { type: 'array', items: { type: 'string' } },
          evidence: { type: 'string' },
        },
        required: ['id', 'verdict', 'check_tests', 'open_issues', 'evidence'],
      },
    },
    fixed: { type: 'array', items: { type: 'string' } },
    disputes: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          test: { type: 'string' },
          ruling: { type: 'string', enum: ['test_correct', 'test_wrong'] },
          reason: { type: 'string' },
        },
        required: ['test', 'ruling', 'reason'],
      },
    },
    failing: FAILURES,
    gate: {
      type: 'object',
      properties: {
        exit_code: { type: 'number' },
        first_failing_step: { type: 'string' },
        tests_passed: { type: 'number' },
        tests_failed: { type: 'number' },
        godot_result: { type: 'string' },
        runs: { type: 'number' },
      },
      required: ['exit_code', 'first_failing_step', 'tests_passed', 'tests_failed', 'godot_result', 'runs'],
    },
  },
  required: ['analysis', 'rows', 'fixed', 'disputes', 'failing', 'gate'],
}
const MERGE = {
  type: 'object',
  properties: {
    merged: { type: 'boolean' },
    merge_commit: { type: 'string' },
    conflicts: { type: 'array', items: { type: 'string' } },
    gate_exit: { type: 'number' },
    first_failing_step: { type: 'string' },
    tests_passed: { type: 'number' },
    cleaned: { type: 'boolean' },
    notes: { type: 'string' },
  },
  required: ['merged', 'merge_commit', 'conflicts', 'gate_exit', 'first_failing_step', 'tests_passed', 'cleaned', 'notes'],
}

const A = args || {}
if (!Array.isArray(A.clusters) || A.clusters.length === 0) throw new Error('args.clusters is empty, run tools/backlog_batch.py prepare')
const ENGINE = A.engine
const CONC = Math.max(1, Math.min(A.concurrency || 9, 12))
const JOBS = A.jobs || 6
const MERGE_ON = A.merge === true
const IDENTITY = A.identity || ''
const CRATES = A.workspace_crates || []
if (CRATES.length === 0) throw new Error('args.workspace_crates is empty, run tools/backlog_batch.py prepare')
const profileOf = c => {
  const p = PROFILES[c.type]
  if (!p) throw new Error(`${c.id}: type must be one of ${Object.keys(PROFILES).join(', ')}`)
  return p
}
A.clusters.forEach(profileOf)

// Opus writes a whole cluster and judges it at medium effort. The judge also fixes, so the code
// it read to judge is the code it repairs, without a fresh agent reading it again. Sonnet breaks,
// so the tests come from a different model than the code they attack.
const cheap = c => c.rows.every(r => r.difficulty === 'low' || r.difficulty === 'medium')
const writerModel = () => 'opus'
const writerEffort = c => (c.rows.some(r => r.difficulty === 'very_high') ? 'high' : 'medium')
const breakerEffort = c => (cheap(c) ? 'medium' : 'high')
const skillPaths = c => profileOf(c).skills.map(s => `${SKILLS}/${s}/SKILL.md`).join(', ')
const checkName = id => id.toLowerCase().replace('-', '_')

// One build directory per lane, reused by the lane's clusters in turn. Cargo fingerprints
// workspace crates by workspace-relative path, so a directory that moves to another worktree
// keeps stale workspace artifacts. Each cluster therefore starts by cleaning the workspace
// packages only, which keeps every dependency built.
const laneDir = lane => `${ENGINE}/target/lanes/lane-${lane}`
const buildEnv = dir =>
  [
    'CARGO_INCREMENTAL=0',
    'CARGO_PROFILE_DEV_DEBUG=0',
    `CARGO_BUILD_JOBS=${JOBS}`,
    A.sccache ? 'RUSTC_WRAPPER=sccache' : '',
    `CARGO_TARGET_DIR=${dir}`,
  ]
    .filter(Boolean)
    .join(' ')

function rules(c, dir) {
  return `Worktree: ${c.worktree} on branch ${c.branch}. Work only there.
Permission rules, nobody answers prompts during this run: never cd. Never &&, || or ; in a Bash command, and never put two commands on separate lines of one call: one command per call. Never delete anything (no rm, no cargo clean) beyond a command this prompt spells out. Absolute paths only, git -C ${c.worktree}, cargo --manifest-path ${c.worktree}/Cargo.toml. Never change files through Bash (no sed -i, mv, cp, rm, redirects or scripted edits), use Read, Edit and Write. Never use EnterWorktree. Never stash, reset, rebase or checkout. Commit only where this prompt says, always as ${IDENTITY} git -C ${c.worktree} commit, with no trailer of any kind.
Every cargo command is prefixed with ${buildEnv(dir)}, this lane's build directory. Build and test only the crates you touch (cargo clippy -p <crate>, cargo test -p <crate> --test <file>). Only the judge and the merge run tools/lint.sh.
Serena and rust-analyzer are available. Load Serena with ToolSearch (query "serena"), call activate_project with ${c.worktree}, then prefer find_symbol, find_referencing_symbols and get_diagnostics_for_file over reading whole files.
House prose in comments, docs and commit messages: no em dashes, no semicolons between clauses, no AI vocabulary.`
}

function rowText(r) {
  return `Row ${r.id}: ${r.title}
  Difficulty ${r.difficulty}. Crates: ${r.crates}. Depends on: ${r.depends_on}.
  Read first: ${r.read_first}
  Done check: ${r.done_check}`
}

function writerPrompt(c, skip, dir) {
  const rows = c.rows.filter(r => !skip.has(r.id))
  return `You implement ${rows.length} related backlog rows in turn, in one worktree. They touch the same subsystem, so explore it once and reuse what you learned for every row. A breaker and a judge test your work afterwards, so write no new tests. Update existing tests only where your change breaks them, and say so.
${rows.map(rowText).join('\n')}
${skip.size > 0 ? `Leave out ${[...skip].join(', ')}: a row they depend on did not merge. Report them with implemented false.\n` : ''}${rules(c, dir)}
Start with exactly these two commands, one per call. The first brings in rows merged earlier in this run: git -C ${c.worktree} merge --ff-only master. The second drops the previous cluster's workspace artifacts from this lane's build directory and keeps its dependencies: ${buildEnv(dir)} cargo clean --manifest-path ${c.worktree}/Cargo.toml ${CRATES.map(n => `-p ${n}`).join(' ')}
Then read ${skillPaths(c)}. Map the subsystem once: the files and symbols these rows touch, the nearest existing implementation to copy the pattern from, the tests to mirror, and every ../engine-refs source (absolute path under ${ENGINE}/../engine-refs) that declares the formats, record layouts or semantics. Grep the corpus before writing anything. ${profileOf(c).write}
Read docs/style.md before the first edit and hold its rules, the gate does not check all of them: typed errors with context and no unwrap or expect outside tests, borrowed parameters, enums over bool flags, // comments only for the why, a safety argument or a tradeoff, tagged CONTEXT: or SAFETY:, never restating the code or the task history, and /// on every public item with # Errors and # Panics where they apply. Match the surrounding code's naming and comment density. In Markdown, put any formula containing an asterisk in a code span. Implement the rows in the order listed. For each row: make the change, update docs/<crate>.md for each crate you touch (what the code does, which sources you read, what remains open), run cargo fmt on the workspace, cargo clippy -p <crate> --all-targets -- -D warnings for each touched crate, comment-lint on every touched crates/<crate> and docs file (new untracked files included), and get_diagnostics_for_file on every touched Rust file. When those pass, commit that row alone: git -C ${c.worktree} add -A, then ${IDENTITY} git -C ${c.worktree} commit -m "<subject>" -m "<body>". The subject is the row as an imperative sentence under 72 characters, the body 2 to 4 sentences on what changed, which sources it follows and what stays open. Report the short hash from git -C ${c.worktree} rev-parse --short HEAD as the row's commit.
If a row's done check cannot be met from this repository and the corpus, for example because it needs observations from the original game, leave it: implemented false and the reason in blocker. A row that depends on a row you left is left too. List every gap between a row's change and its done check in its known_gaps. build_ok is true only if the last row's checks passed.`
}

function breakerPrompt(c, write, dir) {
  const done = write.rows.filter(r => r.implemented)
  const perRow = cheap(c) ? 4 : 6
  return `You are the breaker. Your goal is to break these changes, not to confirm them. Read ${BREAKER_DEF} for the breaking-method catalogue and apply the methods that fit a Rust engine crate. You write test files only, in files ending in _adversarial, and never change the code under test.
${c.rows.filter(r => done.some(d => d.id === r.id)).map(rowText).join('\n')}
${rules(c, dir)}
Do not commit. Each row was committed on its own, so git -C ${c.worktree} show <commit> gives its diff:
${done.map(r => `- ${r.id} at ${r.commit}: ${r.summary} Known gaps: ${r.known_gaps.join('; ') || 'none'}`).join('\n')}
Aim at: ${profileOf(c).break} Also aim at how the rows interact, since they share a subsystem.
Write at most ${perRow} adversarial tests per row, the ones most likely to expose a real defect first, in files named after the row, for example ${checkName(done[0].id)}_<topic>_adversarial. Every bug must be a failing test with the row it belongs to, the expected behaviour and its source (a corpus file, a CK wiki page, or the done check). Assert the correct behaviour, never the current one: a test that passes by pinning the defect is wrong, because the fix then breaks it. Write comments that describe what must hold, not what the code does today. A test whose expectation you cannot source is not a bug, leave it out. Run comment-lint on your files. Report each file with the exact command that runs it.`
}

function judgePrompt(c, write, breakr, dir) {
  const done = write.rows.filter(r => r.implemented)
  const godot = profileOf(c).godot
  return `You are the judge for ${done.length} rows, and you also fix what you find. You run every test yourself, repair the implementation where a test exposes a real defect, write the planned-check test for each row, run the gate, and rule on each row. Agents in this project have reported passing gates over code that did not compile, so trust nothing below that you have not rerun.
${c.rows.filter(r => done.some(d => d.id === r.id)).map(rowText).join('\n')}
${rules(c, dir)}
Row commits (git -C ${c.worktree} show <commit> for each diff):
${done.map(r => `- ${r.id} at ${r.commit}: ${r.summary} Known gaps: ${r.known_gaps.join('; ') || 'none'}`).join('\n')}
Breaker tests: ${breakr.tests.map(t => t.command).join(' | ') || 'none'}
Breaker bugs: ${breakr.bugs.map(b => `${b.row} ${b.test}: expected ${b.expected}, observed ${b.observed} (${b.source})`).join('; ') || 'none'}
Step 1, the breaker's tests. Run every breaker command. For each failing test, read its cited source. If the source contradicts the test's expectation, correct the test's assertion and comment to the sourced contract and record it in disputes as test_wrong. Otherwise it is test_correct: fix the implementation, inside the crates the rows name. Never weaken, delete or skip a test to make it pass. A passing test whose assertion contradicts its cited source locks in a bug: correct it the same way. List what you fixed in fixed.
Step 2, the planned checks. For each row, if no test exercises every clause of its done check, write it: ${profileOf(c).verify} Name the functions after the check id, for example ${checkName(done[0].id)}_..., in a file that does not end in _adversarial.${godot ? ` Godot: build the extension with ${buildEnv(`${dir}/host`)} cargo build --release -p soe-godot-host. Make a disposable project under ${c.worktree}/target/godot-check with Write: project.godot copied from ${c.worktree}/godot, a soe.gdextension whose linux keys point at ${dir}/host/release/libsoe_godot_host.so, and the test script. Run ${GODOT} --headless --path <project> --import once, then --script <test>.gd, with a 20 minute timeout for installed-data runs. Run the same script once on ${GODOT_FLOOR} for the API floor. Copy the RESULT lines into godot_result.` : ' Set godot_result to "n/a".'}
Step 3, the gate. Run ${buildEnv(dir)} ${c.worktree}/tools/lint.sh with a 30 minute timeout. It switches to the worktree root itself and covers formatting, three clippy passes, comment-lint over all of crates and docs, prose checks and the workspace tests. The redundant-function lines about sibling checkouts are informational. If it fails, fix the cause and run it again, at most 3 runs in total. Record the last run's exit code, first failing step or "none", its test counts, and the number of runs. Then rerun every breaker and check command and list what still fails in failing.
Step 4, commit your work only when you changed or added files: git -C ${c.worktree} add -A, then ${IDENTITY} git -C ${c.worktree} commit -m "Fix defects the adversarial tests found in ${c.id}" -m "<which defects, and the planned checks added>". The last gate run must come after your last edit.
Write your reasoning into analysis first, in at most 300 words, then fill every other field. A reply with analysis alone is rejected. Give each row of the cluster a verdict: done only if every clause of its done check is exercised by a passing test you ran and the last gate exited 0, partial when some clauses are met, failed when its change does not work, skipped when the writer left it. evidence is one line per row in the BACKLOG.csv style: what ran and the counts.`
}

function mergePrompt(c, judge, dir) {
  return `You merge one finished cluster branch into master. Clusters merge one at a time, so master does not move while you work. The lead's own uncommitted files may sit in ${ENGINE}: never touch them.
Cluster ${c.id}: ${c.rows.map(r => `${r.id} ${r.title}`).join('; ')}
Worktree: ${c.worktree} on branch ${c.branch}. Engine checkout: ${ENGINE} on master.
Permission rules, nobody answers prompts during this run: never cd. Never &&, || or ; in a Bash command, one command per call. Absolute paths only. Change files only with Read, Edit and Write. You may run the git commands below and nothing destructive beyond them. Commits never carry a trailer of any kind.
Judge gate: exit ${judge.gate.exit_code}, ${judge.gate.tests_passed} passed.
1. Run git -C ${c.worktree} status --porcelain. If it lists files, stop with merged false: the judge left work uncommitted.
2. Run git -C ${c.worktree} merge-base --is-ancestor master HEAD. Exit 0 means master has not moved since the judge's gate, which then covered exactly what will merge: skip to step 5.
3. Bring master in: ${IDENTITY} git -C ${c.worktree} merge master -m "Merge master into ${c.branch}". On conflicts, read both sides and keep both behaviours. When both sides raised SAVE_VERSION in crates/game/src/world/save.rs, master keeps its number and this branch's block takes the next one: raise SAVE_VERSION by one, move this branch's read gate (version >= N) to the new number, update the version it names in its doc comments, docs and tests, and move the unknown-version probe in sav_07_unknown_version_and_mixed_legacy_namespaces_are_rejected (crates/game/src/world/save_tests.rs) to SAVE_VERSION plus one. A test that asserts master's number as the current version reads soe_game::world::SAVE_VERSION instead. List the files in conflicts, then git -C ${c.worktree} add <file> for each and ${IDENTITY} git -C ${c.worktree} commit --no-edit.
4. Gate the merged result: ${buildEnv(dir)} ${c.worktree}/tools/lint.sh with a 30 minute timeout. Record its exit code, first failing step or "none", and the Rust test passes it printed. If it fails because the two sides do not fit together (a struct gained fields on master, a renamed function, a moved import, a test asserting master's old constant), make the smallest edit that restores both behaviours, commit it on the branch, and run the gate once more. If it fails for any other reason, or fails again, stop here: merged false, leave the worktree, explain in notes.
5. Run git -C ${ENGINE} status --porcelain --untracked-files=no. If it lists a file that git -C ${ENGINE} diff --name-only master ${c.branch} also lists, stop with merged false. Otherwise run ${IDENTITY} git -C ${ENGINE} merge --no-ff ${c.branch} -m "Merge branch '${c.branch}'" and report the new master commit from git -C ${ENGINE} rev-parse --short HEAD. When step 2 skipped the gate, gate_exit is the judge's.
6. Clean up after a merge: git -C ${ENGINE} worktree remove --force ${c.worktree}, then git -C ${ENGINE} branch -d ${c.branch}. cleaned is true when both succeed.`
}

const labelOf = (stage, c) => `${stage}:${c.id}`

// Rows other clusters in this run wait on. A waiting row is left out when its dependency did
// not merge, and a cluster with no row left stops before its writer starts.
const clusterOfRow = new Map(A.clusters.flatMap(c => c.rows.map(r => [r.id, c.id])))
const mergedIn = new Map(
  [...clusterOfRow.keys()].map(id => {
    let resolve
    const promise = new Promise(res => (resolve = res))
    return [id, { promise, resolve }]
  }),
)
let mergeChain = Promise.resolve()
function enqueueMerge(c, judge, dir) {
  const run = mergeChain.then(() =>
    agent(mergePrompt(c, judge, dir), {
      label: labelOf('merge', c), phase: 'Merge', model: 'sonnet', effort: 'medium', schema: MERGE,
    }),
  )
  mergeChain = run.catch(() => null)
  return run
}

async function runCluster(c, lane) {
  const dir = laneDir(lane)
  const own = new Set(c.rows.map(r => r.id))
  const external = [...new Set(c.rows.flatMap(r => r.depends_on.split(',')).filter(d => clusterOfRow.has(d) && !own.has(d)))]
  const failedDeps = new Set()
  if (external.length > 0) {
    if (!MERGE_ON) return { c, stopped: 'dependency', error: `${external.join(', ')} in the same run without merging` }
    const ok = await Promise.all(external.map(d => mergedIn.get(d).promise))
    external.forEach((d, i) => { if (!ok[i]) failedDeps.add(d) })
  }
  const skip = new Set(c.rows.filter(r => r.depends_on.split(',').some(d => failedDeps.has(d))).map(r => r.id))
  if (skip.size === c.rows.length) return { c, stopped: 'dependency', error: `${[...failedDeps].join(', ')} did not merge` }

  const write = await agent(writerPrompt(c, skip, dir), {
    label: labelOf('write', c), phase: 'Write', model: writerModel(c), effort: writerEffort(c), schema: WRITE,
  })
  if (!write || !write.build_ok || !write.rows.some(r => r.implemented)) return { c, stopped: 'write', write }
  const breakr = await agent(breakerPrompt(c, write, dir), {
    label: labelOf('break', c), phase: 'Break', model: 'sonnet', effort: breakerEffort(c), schema: BREAK,
  })
  if (!breakr) return { c, stopped: 'break', write }
  const judge = await agent(judgePrompt(c, write, breakr, dir), {
    label: labelOf('judge', c), phase: 'Judge', model: 'opus', effort: 'medium', schema: JUDGE,
  })
  if (!judge) return { c, stopped: 'judge', write, breakr }
  log(`${c.id}: ${judge.rows.map(r => `${r.id} ${r.verdict}`).join(', ')}, gate exit ${judge.gate.exit_code} after ${judge.gate.runs} runs`)
  const implemented = judge.rows.filter(r => r.verdict !== 'skipped')
  const allDone = implemented.length > 0 && implemented.every(r => r.verdict === 'done') && judge.gate.exit_code === 0
  let merge = null
  if (MERGE_ON && allDone) merge = await enqueueMerge(c, judge, dir)
  return { c, write, breakr, judge, merge }
}

// Lane k takes clusters k, k + CONC, k + 2 * CONC and so on, one after another. prepare orders
// clusters so none waits on a later one, which keeps a lane from waiting on itself.
const lanes = Array.from({ length: Math.min(CONC, A.clusters.length) }, (_, k) => A.clusters.filter((_, i) => i % CONC === k))
const rowCount = A.clusters.reduce((n, c) => n + c.rows.length, 0)
log(`${rowCount} rows in ${A.clusters.length} clusters over ${lanes.length} lanes, ${JOBS} cargo jobs per lane${A.sccache ? ', sccache on' : ''}${MERGE_ON ? ', merging as clusters finish' : ''}`)
const laneResults = await parallel(
  lanes.map((clusters, k) => async () => {
    const out = []
    for (const c of clusters) {
      let result
      try {
        result = await runCluster(c, k)
      } catch (e) {
        result = { c, stopped: 'error', error: String(e) }
      }
      const merged = Boolean(result.merge && result.merge.merged)
      for (const r of c.rows) {
        const verdict = result.judge ? (result.judge.rows.find(j => j.id === r.id) || {}).verdict : null
        mergedIn.get(r.id).resolve(merged && verdict === 'done')
      }
      out.push(result)
    }
    return out
  }),
)
const byId = new Map(laneResults.filter(Boolean).flat().map(s => [s.c.id, s]))

return A.clusters.flatMap(c => {
  const s = byId.get(c.id)
  if (!s) return c.rows.map(r => ({ id: r.id, cluster: c.id, worktree: c.worktree, verdict: 'error', note: 'lane failed' }))
  if (s.stopped) {
    return c.rows.map(r => {
      const w = s.write ? s.write.rows.find(x => x.id === r.id) : null
      return { id: r.id, cluster: c.id, worktree: c.worktree, verdict: `stopped at ${s.stopped}`, blocker: w ? w.blocker : s.error || '', known_gaps: w ? w.known_gaps : [] }
    })
  }
  return c.rows.map(r => {
    const j = s.judge.rows.find(x => x.id === r.id) || { verdict: 'missing', evidence: '', open_issues: [] }
    const w = s.write.rows.find(x => x.id === r.id) || {}
    return {
      id: r.id,
      cluster: c.id,
      branch: c.branch,
      worktree: c.worktree,
      verdict: j.verdict,
      evidence: j.evidence,
      open_issues: j.open_issues,
      blocker: w.blocker || '',
      commit: w.commit || '',
      gate: s.judge.gate,
      failing: s.judge.failing,
      disputes: s.judge.disputes,
      bugs_found: s.breakr.bugs.filter(b => b.row === r.id).length,
      merge: s.merge ? { merged: s.merge.merged, commit: s.merge.merge_commit, notes: s.merge.notes } : null,
    }
  })
})
