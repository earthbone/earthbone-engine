export const meta = {
  name: 'soe-backlog',
  description: 'Implement prepared BACKLOG.csv rows: haiku scout, writer, breaker, fix loop, and a judge that writes the done check and runs the gate',
  whenToUse: 'After tools/backlog_batch.py prepare created worktrees. Pass its JSON as args. Runs args.concurrency lanes, each with its own reusable build directory.',
  phases: [
    { title: 'Scout', detail: 'haiku maps files, symbols and corpus sources into a context pack', model: 'haiku' },
    { title: 'Write', detail: 'sonnet implements, opus for very_high rows' },
    { title: 'Break', detail: 'breaker writes adversarial tests' },
    { title: 'Fix', detail: 'fixer repairs without weakening tests and reruns them' },
    { title: 'Judge', detail: 'writes the planned check, runs the gate, rules on disputes' },
    { title: 'Merge', detail: 'one row at a time: commit, merge master in, gate, merge, clean up' },
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

const SCOUT = {
  type: 'object',
  properties: {
    feasible: { type: 'boolean' },
    blocker: { type: 'string' },
    pack_path: { type: 'string' },
    files: { type: 'array', items: { type: 'string' } },
    corpus_sources: { type: 'array', items: { type: 'string' } },
  },
  required: ['feasible', 'blocker', 'pack_path', 'files', 'corpus_sources'],
}
const WRITE = {
  type: 'object',
  properties: {
    build_ok: { type: 'boolean' },
    summary: { type: 'string' },
    files_changed: { type: 'array', items: { type: 'string' } },
    known_gaps: { type: 'array', items: { type: 'string' } },
  },
  required: ['build_ok', 'summary', 'files_changed', 'known_gaps'],
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
const BREAK = {
  type: 'object',
  properties: {
    tests: TESTS,
    bugs: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          test: { type: 'string' },
          severity: { type: 'string', enum: ['high', 'medium', 'low'] },
          expected: { type: 'string' },
          observed: { type: 'string' },
          source: { type: 'string' },
        },
        required: ['test', 'severity', 'expected', 'observed', 'source'],
      },
    },
  },
  required: ['tests', 'bugs'],
}
const FIX = {
  type: 'object',
  properties: {
    fixed: { type: 'array', items: { type: 'string' } },
    disputed: {
      type: 'array',
      items: {
        type: 'object',
        properties: { test: { type: 'string' }, reason: { type: 'string' } },
        required: ['test', 'reason'],
      },
    },
    still_failing: FAILURES,
    files_changed: { type: 'array', items: { type: 'string' } },
  },
  required: ['fixed', 'disputed', 'still_failing', 'files_changed'],
}
const JUDGE = {
  type: 'object',
  properties: {
    analysis: { type: 'string' },
    verdict: { type: 'string', enum: ['done', 'partial', 'failed'] },
    check_tests: TESTS,
    failing: FAILURES,
    gate: {
      type: 'object',
      properties: {
        exit_code: { type: 'number' },
        first_failing_step: { type: 'string' },
        tests_passed: { type: 'number' },
        tests_failed: { type: 'number' },
        godot_result: { type: 'string' },
      },
      required: ['exit_code', 'first_failing_step', 'tests_passed', 'tests_failed', 'godot_result'],
    },
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
    open_issues: { type: 'array', items: { type: 'string' } },
    evidence: { type: 'string' },
  },
  required: ['analysis', 'verdict', 'check_tests', 'failing', 'gate', 'disputes', 'open_issues', 'evidence'],
}

const MERGE = {
  type: 'object',
  properties: {
    merged: { type: 'boolean' },
    branch_commit: { type: 'string' },
    merge_commit: { type: 'string' },
    conflicts: { type: 'array', items: { type: 'string' } },
    gate_exit: { type: 'number' },
    first_failing_step: { type: 'string' },
    tests_passed: { type: 'number' },
    cleaned: { type: 'boolean' },
    notes: { type: 'string' },
  },
  required: ['merged', 'branch_commit', 'merge_commit', 'conflicts', 'gate_exit', 'first_failing_step', 'tests_passed', 'cleaned', 'notes'],
}

const A = args || {}
if (!Array.isArray(A.rows) || A.rows.length === 0) throw new Error('args.rows is empty, run tools/backlog_batch.py prepare')
const ENGINE = A.engine
const CONC = Math.max(1, Math.min(A.concurrency || 9, 12))
const JOBS = A.jobs || 6
const MERGE_ON = A.merge === true
const IDENTITY = A.identity || ''
const CRATES = A.workspace_crates || []
if (CRATES.length === 0) throw new Error('args.workspace_crates is empty, run tools/backlog_batch.py prepare')
const profileOf = row => {
  const p = PROFILES[row.type]
  if (!p) throw new Error(`${row.id}: type must be one of ${Object.keys(PROFILES).join(', ')}`)
  return p
}
A.rows.forEach(profileOf)

// Effort follows difficulty. Opus writes very_high rows and runs the later fix rounds.
const cheap = row => row.difficulty === 'low' || row.difficulty === 'medium'
const writerModel = row => (row.difficulty === 'very_high' ? 'opus' : 'sonnet')
const writerEffort = row => (row.difficulty === 'low' ? 'low' : row.effort === 'medium' ? 'medium' : 'high')
const breakerEffort = row => (cheap(row) ? 'medium' : 'high')
// Low-effort judges returned only their analysis and never the verdict fields, four times out of four.
const judgeEffort = row => (cheap(row) ? 'medium' : 'high')
const skillPaths = row => profileOf(row).skills.map(s => `${SKILLS}/${s}/SKILL.md`).join(', ')

// One build directory per lane, reused by the lane's rows in turn. Cargo fingerprints workspace
// crates by workspace-relative path, so a directory that moves to another worktree keeps stale
// workspace artifacts. Each row therefore starts by cleaning the workspace packages only, which
// keeps every dependency built. No incremental state and no debug info keep a lane to a few GiB.
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

function rules(row, dir) {
  return `Worktree: ${row.worktree} on branch ${row.branch}. Work only there.
Permission rules, nobody answers prompts during this run: never cd. Never &&, || or ; in a Bash command, and never put two commands on separate lines of one call: one command per call. Never delete anything (no rm, no cargo clean) beyond a command this prompt spells out. Absolute paths only, git -C ${row.worktree}, cargo --manifest-path ${row.worktree}/Cargo.toml. Never change files through Bash (no sed -i, mv, cp, rm, redirects or scripted edits), use Read, Edit and Write. Never use EnterWorktree. Never commit, stage, stash, reset or checkout: the lead reviews and commits.
Every cargo command is prefixed with ${buildEnv(dir)}, this lane's build directory. Build and test only the crates you touch (cargo clippy -p <crate>, cargo test -p <crate> --test <file>). Only the judge runs tools/lint.sh.
Serena and rust-analyzer are available. Load Serena with ToolSearch (query "serena"), call activate_project with ${row.worktree}, then prefer find_symbol, find_referencing_symbols and get_diagnostics_for_file over reading whole files.`
}

function rowText(row) {
  return `Row ${row.id}: ${row.title}
Difficulty ${row.difficulty}. Crates: ${row.crates}. Depends on: ${row.depends_on}.
Read first: ${row.read_first}
Done check: ${row.done_check}`
}

function scoutPrompt(row, dir) {
  return `You map the codebase for one backlog row so the writer starts with a small context pack instead of exploring.
${rowText(row)}
${rules(row, dir)}
First run exactly these two commands, one per call. The first brings in rows merged earlier in this run: git -C ${row.worktree} merge --ff-only master. The second drops the previous row's workspace artifacts from this lane's build directory and keeps its dependencies: ${buildEnv(dir)} cargo clean --manifest-path ${row.worktree}/Cargo.toml ${CRATES.map(c => `-p ${c}`).join(' ')}
Then read ${skillPaths(row)} for where answers live. Find: the files and symbols the change touches (with line ranges), the nearest existing implementation to copy the pattern from, the existing tests to mirror, and every ../engine-refs source (absolute path under ${ENGINE}/../engine-refs) that declares the format, record layout or semantics. Grep the corpus before anything else.
Write the pack with Write to ${row.worktree}/target/task-pack/${row.id}.md: the row, a file list with line ranges and one line on why each matters, corpus sources with the exact file and section, test files to mirror, and open questions. Keep it under 250 lines. Do not propose code.
Set feasible to false only when the done check cannot be met from this repository and the corpus, for example when it needs observations from the original game, and put the reason in blocker. Otherwise blocker is empty.`
}

function writerPrompt(row, scout, dir) {
  return `You implement one backlog row. A breaker and a judge test your work afterwards, so write no new tests. Update existing tests only where your change breaks them, and say so.
${rowText(row)}
${rules(row, dir)}
Start with the context pack at ${scout.pack_path}, then the skills at ${skillPaths(row)}. ${profileOf(row).write}
Update docs/<crate>.md for each crate you touch: what the code does, which sources you read, what remains open. House prose: no em dashes, no semicolons between clauses, no AI vocabulary.
Before returning, run cargo fmt on the workspace, cargo clippy -p <crate> --all-targets -- -D warnings for each touched crate, comment-lint on every touched crates/<crate> and docs file (new untracked files included), and get_diagnostics_for_file on every touched Rust file. build_ok is true only if those pass. List every gap between your change and the done check in known_gaps.`
}

function breakerPrompt(row, scout, write, dir) {
  return `You are the breaker. Your goal is to break this change, not to confirm it. Read ${BREAKER_DEF} for the breaking-method catalogue and apply the methods that fit a Rust engine crate. You write test files only, in files ending in _adversarial, and never change the code under test.
${rowText(row)}
${rules(row, dir)}
Context pack: ${scout.pack_path}. Writer summary: ${write.summary}
Files changed: ${write.files_changed.join(', ')}. Writer's known gaps: ${write.known_gaps.join('; ') || 'none'}
Aim at: ${profileOf(row).break}
Write at most ${cheap(row) ? 6 : 12} adversarial tests, the ones most likely to expose a real defect first. Every bug must be a failing test with the expected behaviour and its source (a corpus file, a CK wiki page, or the done check). Assert the correct behaviour, never the current one: a test that passes by pinning the defect ("current reader", "documents the bug") is wrong, because the fix then breaks it. Write comments that describe what must hold, not what the code does today. A test whose expectation you cannot source is not a bug, leave it out. Run comment-lint on your files. Report each file with the exact command that runs it.`
}

function fixerPrompt(row, scout, failures, tests, round, dir) {
  return `You are the fixer, round ${round}. Tests fail against the writer's change. First check the written code against the context pack and the skills, and correct any deviation. Then make the failing tests pass by fixing the implementation. Never weaken, delete or skip a test to make it pass. If a test's expectation contradicts its cited source, leave it failing and list it under disputed with the source that contradicts it. A judge rules on disputes.
${rowText(row)}
${rules(row, dir)}
Context pack: ${scout.pack_path}. Skills: ${skillPaths(row)}
Failing tests:
${failures.map(f => `- ${f.test}: ${f.failure}`).join('\n')}
Keep the change inside the crates the row names. Run cargo fmt, clippy -p <crate> --all-targets -- -D warnings and comment-lint on every file you or the test writers touched. Then run each of these commands and report every test that still fails in still_failing:
${tests.map(t => `- ${t.command}`).join('\n')}
A test that asserted the pre-fix behaviour now fails against the fixed code: correct its assertion and comment to the fixed contract and list it under fixed.`
}

function judgePrompt(row, s, dir) {
  const disputes = s.fixes.flatMap(f => (f ? f.disputed : []))
  const godot = profileOf(row).godot
  return `You are the judge. You also do the verifier's job: you write the test for the row's planned check, then decide from your own runs whether the row meets its done check. Agents in this project have reported passing gates over code that did not compile, so trust nothing below that you have not rerun. You write test files only and never change the code under test.
${rowText(row)}
${rules(row, dir)}
Writer summary: ${s.write.summary}. Writer's known gaps: ${s.write.known_gaps.join('; ') || 'none'}
Breaker tests: ${s.breakr.tests.map(t => t.command).join(' | ') || 'none'}
Remaining failures after fixing: ${s.remaining.map(f => f.test).join(', ') || 'none'}
Disputed tests: ${disputes.map(d => `${d.test} (${d.reason})`).join('; ') || 'none'}
Step 1, the planned check. Run git -C ${row.worktree} diff master --stat and read the diff. If no test exercises every clause of the done check yet, write it: ${profileOf(row).verify} Name the functions after the check id, for example ${row.id.toLowerCase().replace('-', '_')}_..., in a file that does not end in _adversarial. Report your check test files with their commands in check_tests.${godot ? ` Godot: build the extension with ${buildEnv(`${dir}/host`)} cargo build --release -p soe-godot-host. Make a disposable project under ${row.worktree}/target/godot-check with Write: project.godot copied from ${row.worktree}/godot, a soe.gdextension whose linux keys point at ${dir}/host/release/libsoe_godot_host.so, and the test script. Run ${GODOT} --headless --path <project> --import once, then --script <test>.gd, with a 20 minute timeout for installed-data runs. Run the same script once on ${GODOT_FLOOR} for the API floor. Copy the RESULT line into godot_result.` : ' Set godot_result to "n/a".'}
Step 2, the gate. Run the project gate exactly as the lead will: ${buildEnv(dir)} ${row.worktree}/tools/lint.sh with a 30 minute timeout. It switches to the worktree root itself. It covers formatting, three clippy passes, comment-lint over all of crates and docs (new untracked files included), prose checks and the workspace tests. The redundant-function lines about sibling checkouts are informational. Record its exit code and its first failing step, or "none". Then run every breaker and check test command. List every test that fails in failing.
Step 3, rulings. For each disputed test, read its cited source and rule test_correct or test_wrong.
Write your reasoning into analysis first, in at most 250 words, then fill every other field. A reply with analysis alone is rejected. verdict is done only if every clause of the done check is exercised by a passing test you ran, the gate exits 0, and no test ruled correct still fails. A passing test whose assertion contradicts its cited source locks in a bug: rule it test_wrong and the verdict can be at most partial. Use partial when some clauses are met, failed when the change does not work. evidence is one line in the BACKLOG.csv style: what ran and the counts.`
}

function mergePrompt(row, s, dir) {
  return `You merge one finished backlog row into master. Rows merge one at a time, so master does not move while you work. The lead's own uncommitted files may sit in ${ENGINE}: never touch them.
Row ${row.id}: ${row.title}
Worktree: ${row.worktree} on branch ${row.branch}. Engine checkout: ${ENGINE} on master.
Permission rules, nobody answers prompts during this run: never cd. Never &&, || or ; in a Bash command, one command per call. Absolute paths only. Change files only with Read, Edit and Write. You may run the git commands below and nothing destructive beyond them.
Judge evidence: ${s.judge.evidence}
1. Commit the row on its branch. Run git -C ${row.worktree} add -A, then ${IDENTITY} git -C ${row.worktree} commit -m "<subject>" -m "<body>". The subject is the row as an imperative sentence under 72 characters. The body is 2 to 4 sentences: what changed, which sources it follows, what stays open. House prose: no em dashes, no semicolons between clauses, no AI vocabulary, and no trailer of any kind (no Co-Authored-By, no Generated-with line).
2. Bring master in: ${IDENTITY} git -C ${row.worktree} merge master -m "Merge master into ${row.branch}". On conflicts, read both sides and keep both behaviours. When both sides raised SAVE_VERSION in crates/game/src/world/save.rs, master keeps its number and this row's block takes the next one: raise SAVE_VERSION by one, move this row's read gate (version >= N) to the new number, update the version this row names in its doc comments, docs and tests, and move the unknown-version probe in sav_07_unknown_version_and_mixed_legacy_namespaces_are_rejected (crates/game/src/world/save_tests.rs) to SAVE_VERSION plus one. A test that asserts master's number as the current version reads soe_game::world::SAVE_VERSION instead. List the files in conflicts, then git -C ${row.worktree} add <file> for each and ${IDENTITY} git -C ${row.worktree} commit --no-edit.
3. Gate the merged result: ${buildEnv(dir)} ${row.worktree}/tools/lint.sh with a 30 minute timeout. Record its exit code, first failing step or "none", and the Rust test passes it printed. If it fails, stop here: merged false, leave the worktree, explain in notes.
4. Only with gate exit 0: run git -C ${ENGINE} status --porcelain --untracked-files=no. If it lists a file that git -C ${ENGINE} diff --name-only master ${row.branch} also lists, stop with merged false. Otherwise run ${IDENTITY} git -C ${ENGINE} merge --no-ff ${row.branch} -m "Merge branch '${row.branch}'" and report the new master commit from git -C ${ENGINE} rev-parse --short HEAD.
5. Clean up after a merge: git -C ${ENGINE} worktree remove --force ${row.worktree}, then git -C ${ENGINE} branch -d ${row.branch}. cleaned is true when both succeed.`
}

const labelOf = (stage, row) => `${stage}:${row.id}`

// Rows in this run that others wait on. A dependent row starts only after its dependencies
// merged in this run, and stops when one of them did not.
const inRun = new Set(A.rows.map(r => r.id))
const mergedIn = new Map(
  A.rows.map(r => {
    let resolve
    const promise = new Promise(res => (resolve = res))
    return [r.id, { promise, resolve }]
  }),
)
let mergeChain = Promise.resolve()
function enqueueMerge(row, s, dir) {
  const run = mergeChain.then(() =>
    agent(mergePrompt(row, s, dir), {
      label: labelOf('merge', row), phase: 'Merge', model: 'sonnet', effort: 'medium', schema: MERGE,
    }),
  )
  mergeChain = run.catch(() => null)
  return run
}

async function runRow(row, lane) {
  const dir = laneDir(lane)
  const waits = row.depends_on.split(',').filter(d => inRun.has(d))
  if (waits.length > 0) {
    if (!MERGE_ON) return { row, stopped: 'dependency', error: `${waits.join(', ')} in the same run without merging` }
    const ok = await Promise.all(waits.map(d => mergedIn.get(d).promise))
    const missing = waits.filter((_, i) => !ok[i])
    if (missing.length > 0) return { row, stopped: 'dependency', error: `${missing.join(', ')} did not merge` }
  }
  const scout = await agent(scoutPrompt(row, dir), {
    label: labelOf('scout', row), phase: 'Scout', model: 'haiku', effort: 'low', schema: SCOUT,
  })
  if (!scout || !scout.feasible) return { row, stopped: 'scout', scout }
  const write = await agent(writerPrompt(row, scout, dir), {
    label: labelOf('write', row), phase: 'Write', model: writerModel(row), effort: writerEffort(row), schema: WRITE,
  })
  if (!write || !write.build_ok) return { row, stopped: 'write', scout, write }
  const breakr = await agent(breakerPrompt(row, scout, write, dir), {
    label: labelOf('break', row), phase: 'Break', model: 'sonnet', effort: breakerEffort(row), schema: BREAK,
  })
  if (!breakr) return { row, stopped: 'break', scout, write }

  const fixModel = round => (round >= 2 || row.difficulty === 'very_high' ? 'opus' : 'sonnet')
  const fixes = []
  let failing = breakr.bugs.map(b => ({ test: b.test, failure: `expected ${b.expected}, observed ${b.observed} (${b.source})` }))
  for (let round = 1; round <= 2 && failing.length > 0; round++) {
    const fix = await agent(fixerPrompt(row, scout, failing, breakr.tests, round, dir), {
      label: labelOf(`fix${round}`, row), phase: 'Fix', model: fixModel(round),
      effort: cheap(row) && round === 1 ? 'medium' : 'high', schema: FIX,
    })
    fixes.push(fix)
    if (!fix) break
    const disputed = new Set(fix.disputed.map(d => d.test))
    failing = fix.still_failing.filter(f => !disputed.has(f.test))
    log(`${row.id} fix round ${round}: ${fix.still_failing.length} failing, ${disputed.size} disputed`)
  }

  const judgeOpts = n => ({
    label: labelOf(n === 1 ? 'judge' : 'rejudge', row), phase: 'Judge', model: 'sonnet', effort: judgeEffort(row), schema: JUDGE,
  })
  let judge = await agent(judgePrompt(row, { write, breakr, fixes, remaining: failing }, dir), judgeOpts(1))
  // One repair round when the judge's own check or the gate finds failures the fix loop never saw.
  if (judge && judge.verdict !== 'done' && judge.failing.length > 0) {
    const tests = [...breakr.tests, ...judge.check_tests]
    const fix = await agent(fixerPrompt(row, scout, judge.failing, tests, fixes.length + 1, dir), {
      label: labelOf('repair', row), phase: 'Fix', model: fixModel(fixes.length + 1), effort: 'high', schema: FIX,
    })
    fixes.push(fix)
    if (fix) {
      judge = await agent(judgePrompt(row, { write, breakr, fixes, remaining: fix.still_failing }, dir), judgeOpts(2))
    }
  }
  let merge = null
  if (MERGE_ON && judge && judge.verdict === 'done') merge = await enqueueMerge(row, { judge }, dir)
  return { row, scout, write, breakr, fixes, judge, merge }
}

// Lane k takes rows k, k + CONC, k + 2 * CONC and so on, one after another. The assignment is fixed
// by row order, so a resumed run builds the same prompts and replays finished agents from cache.
const lanes = Array.from({ length: Math.min(CONC, A.rows.length) }, (_, k) => A.rows.filter((_, i) => i % CONC === k))
log(`${A.rows.length} rows over ${lanes.length} lanes, ${JOBS} cargo jobs per lane${A.sccache ? ', sccache on' : ''}${MERGE_ON ? ', merging as rows finish' : ''}`)
const laneResults = await parallel(
  lanes.map((rows, k) => async () => {
    const out = []
    for (const row of rows) {
      let result
      try {
        result = await runRow(row, k)
      } catch (e) {
        result = { row, stopped: 'error', error: String(e) }
      }
      mergedIn.get(row.id).resolve(Boolean(result.merge && result.merge.merged))
      out.push(result)
    }
    return out
  }),
)
const byId = new Map(laneResults.filter(Boolean).flat().map(s => [s.row.id, s]))

return A.rows.map(row => {
  const s = byId.get(row.id)
  if (!s) return { id: row.id, worktree: row.worktree, verdict: 'error', note: 'lane failed' }
  if (s.stopped) {
    return {
      id: row.id,
      worktree: row.worktree,
      verdict: `stopped at ${s.stopped}`,
      blocker: s.scout ? s.scout.blocker : s.error || '',
      write: s.write ? { build_ok: s.write.build_ok, known_gaps: s.write.known_gaps } : null,
    }
  }
  return {
    id: row.id,
    branch: row.branch,
    worktree: row.worktree,
    verdict: s.judge ? s.judge.verdict : 'no judge result',
    evidence: s.judge ? s.judge.evidence : '',
    gate: s.judge ? s.judge.gate : null,
    failing: s.judge ? s.judge.failing : [],
    open_issues: s.judge ? s.judge.open_issues : [],
    disputes: s.judge ? s.judge.disputes : [],
    bugs_found: s.breakr.bugs.length,
    fix_rounds: s.fixes.length,
    files_changed: s.write.files_changed,
    merge: s.merge,
  }
})
