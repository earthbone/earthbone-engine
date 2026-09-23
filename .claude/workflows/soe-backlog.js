export const meta = {
  name: 'soe-backlog',
  description: 'Implement prepared BACKLOG.csv rows: haiku scout, writer, verifier and breaker, fix loop, independent judge',
  whenToUse: 'After tools/backlog_batch.py prepare created worktrees. Pass its JSON as args. Profiles: native, gameplay, format, host, shim, tooling.',
  phases: [
    { title: 'Scout', detail: 'haiku maps files, symbols and corpus sources into a context pack', model: 'haiku' },
    { title: 'Write', detail: 'sonnet implements, opus for very_high rows' },
    { title: 'Check', detail: 'verifier writes the planned check, breaker writes adversarial tests' },
    { title: 'Fix', detail: 'fixer repairs without weakening tests, haiku reruns the tests' },
    { title: 'Judge', detail: 'independent done-check verdict with its own gate run' },
  ],
}

const SKILLS = '/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/.claude/skills'
const BREAKER_DEF = '/var/home/pengolodh/.claude/agents/breaker.md'
const GODOT = '/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/tools/godot/Godot_v4.7.2-stable_linux.x86_64'
const GODOT_FLOOR = '/run/host/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/tools/godot/Godot_v4.4.1-stable_linux.x86_64'
const DATA = '/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data'

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
const VERIFY = {
  type: 'object',
  properties: { tests: TESTS, failing: FAILURES, done_check_covered: { type: 'boolean' }, notes: { type: 'string' } },
  required: ['tests', 'failing', 'done_check_covered', 'notes'],
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
    files_changed: { type: 'array', items: { type: 'string' } },
  },
  required: ['fixed', 'disputed', 'files_changed'],
}
const RECHECK = {
  type: 'object',
  properties: { failing: FAILURES, passed: { type: 'number' } },
  required: ['failing', 'passed'],
}
const JUDGE = {
  type: 'object',
  properties: {
    analysis: { type: 'string' },
    verdict: { type: 'string', enum: ['done', 'partial', 'failed'] },
    gate: {
      type: 'object',
      properties: {
        fmt: { type: 'boolean' },
        clippy: { type: 'boolean' },
        comment_lint: { type: 'boolean' },
        tests_passed: { type: 'number' },
        tests_failed: { type: 'number' },
        godot_result: { type: 'string' },
      },
      required: ['fmt', 'clippy', 'comment_lint', 'tests_passed', 'tests_failed', 'godot_result'],
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
  required: ['analysis', 'verdict', 'gate', 'disputes', 'open_issues', 'evidence'],
}

const A = args || {}
const P = PROFILES[A.type]
if (!P) throw new Error(`args.type must be one of ${Object.keys(PROFILES).join(', ')}`)
if (!Array.isArray(A.rows) || A.rows.length === 0) throw new Error('args.rows is empty, run tools/backlog_batch.py prepare')
if (A.rows.length > 3) throw new Error('at most three code forks run at once')
const ENGINE = A.engine

const writerModel = row => (row.difficulty === 'very_high' ? 'opus' : 'sonnet')
const writerEffort = row => (row.effort === 'medium' ? 'medium' : 'high')
const breakerEffort = row => (row.difficulty === 'medium' ? 'medium' : 'high')
const skillPaths = () => P.skills.map(s => `${SKILLS}/${s}/SKILL.md`).join(', ')
const releaseDir = row => `${ENGINE}/target/host-${row.branch}`

function rules(row) {
  return `Worktree: ${row.worktree} on branch ${row.branch}. Work only there.
Permission rules, nobody answers prompts during this run: never cd. Never &&, || or ; in a Bash command, one command per call. Absolute paths only, git -C ${row.worktree}, cargo --manifest-path ${row.worktree}/Cargo.toml. Never change files through Bash (no sed -i, mv, cp, rm, redirects or scripted edits), use Read, Edit and Write. Never use EnterWorktree. Never commit, stage, stash, reset or checkout: the lead reviews and commits.
Every cargo command is prefixed with CARGO_TARGET_DIR=${ENGINE}/target so the batch reuses one build directory.
Serena and rust-analyzer are available. Load Serena with ToolSearch (query "serena"), call activate_project with ${row.worktree}, then prefer find_symbol, find_referencing_symbols and get_diagnostics_for_file over reading whole files.`
}

function rowText(row) {
  return `Row ${row.id}: ${row.title}
Difficulty ${row.difficulty}. Crates: ${row.crates}. Depends on: ${row.depends_on}.
Read first: ${row.read_first}
Done check: ${row.done_check}`
}

function scoutPrompt(row) {
  return `You map the codebase for one backlog row so the writer starts with a small context pack instead of exploring.
${rowText(row)}
${rules(row)}
Read ${skillPaths()} for where answers live. Then find: the files and symbols the change touches (with line ranges), the nearest existing implementation to copy the pattern from, the existing tests to mirror, and every ../engine-refs source (absolute path under ${ENGINE}/../engine-refs) that declares the format, record layout or semantics. Grep the corpus before anything else.
Write the pack with Write to ${row.worktree}/target/task-pack/${row.id}.md: the row, a file list with line ranges and one line on why each matters, corpus sources with the exact file and section, test files to mirror, and open questions. Keep it under 250 lines. Do not propose code.
Set feasible to false only when the done check cannot be met from this repository and the corpus, for example when it needs observations from the original game, and put the reason in blocker. Otherwise blocker is empty.`
}

function writerPrompt(row, scout) {
  return `You implement one backlog row. A verifier and a breaker test your work afterwards, so write no new tests. Update existing tests only where your change breaks them, and say so.
${rowText(row)}
${rules(row)}
Start with the context pack at ${scout.pack_path}, then the skills at ${skillPaths()}. ${P.write}
Update docs/<crate>.md for each crate you touch: what the code does, which sources you read, what remains open. House prose: no em dashes, no semicolons between clauses, no AI vocabulary.
Before returning, run cargo fmt on the workspace, cargo clippy -p <crate> --all-targets -- -D warnings for each touched crate, and get_diagnostics_for_file on every touched Rust file. build_ok is true only if those pass. List every gap between your change and the done check in known_gaps.`
}

function verifierPrompt(row, scout, write) {
  return `You are the verifier. You establish that the change meets its done check on the documented path. You write test files only and never change the code under test.
${rowText(row)}
${rules(row)}
Context pack: ${scout.pack_path}. Writer summary: ${write.summary}
Files changed: ${write.files_changed.join(', ')}
${P.verify}
Name the test functions after the check id, for example ${row.id.toLowerCase().replace('-', '_')}_..., and keep them in files the breaker does not use (the breaker writes files ending in _adversarial).
${P.godot ? `Godot: build the extension with CARGO_TARGET_DIR=${releaseDir(row)} cargo build --release -p soe-godot-host. Make a disposable project under ${row.worktree}/target/godot-check with Write: project.godot copied from ${row.worktree}/godot, a soe.gdextension whose linux keys point at ${releaseDir(row)}/release/libsoe_godot_host.so, and the test script. Run ${GODOT} --headless --path <project> --import once, then --script <test>.gd. Installed-data runs take minutes, give them a 20 minute timeout. Run the same script once on ${GODOT_FLOOR} for the API floor. Put each Godot command in tests[].command.` : ''}
Run every test you added. Report each file with the exact command that runs it, and each failing test with its failure. done_check_covered is true only if a passing test exercises every clause of the done check.`
}

function breakerPrompt(row, scout, write) {
  return `You are the breaker. Your goal is to break this change, not to confirm it. Read ${BREAKER_DEF} for the breaking-method catalogue and apply the methods that fit a Rust engine crate. You write test files only, in files ending in _adversarial, and never change the code under test.
${rowText(row)}
${rules(row)}
Context pack: ${scout.pack_path}. Writer summary: ${write.summary}
Files changed: ${write.files_changed.join(', ')}. Writer's known gaps: ${write.known_gaps.join('; ') || 'none'}
Aim at: ${P.break}
Every bug must be a failing test with the expected behaviour and its source (a corpus file, a CK wiki page, or the done check). A test whose expectation you cannot source is not a bug, leave it out. Report each file with the exact command that runs it.`
}

function fixerPrompt(row, scout, write, failures, round) {
  return `You are the fixer, round ${round}. Tests written by a verifier and a breaker fail against the writer's change. First check the written code against the context pack and the skills, and correct any deviation. Then make the failing tests pass by fixing the implementation. Never weaken, delete, skip or rewrite a test. If a test's expectation contradicts its cited source, leave it failing and list it under disputed with the source that contradicts it. A judge rules on disputes.
${rowText(row)}
${rules(row)}
Context pack: ${scout.pack_path}. Skills: ${skillPaths()}
Failing tests:
${failures.map(f => `- ${f.test}: ${f.failure}`).join('\n')}
Keep the change inside the crates the row names. Run cargo fmt, clippy -p <crate> --all-targets -- -D warnings and the failing tests before returning.`
}

function recheckPrompt(row, commands) {
  return `Run these test commands in order and report every failing test with a one-line failure. Do not edit any file.
${rules(row)}
${commands.map(c => `- ${c}`).join('\n')}
passed is the total number of passing tests across all commands.`
}

function judgePrompt(row, s) {
  const disputes = s.fixes.flatMap(f => (f.fix ? f.fix.disputed : []))
  return `You are the judge. Decide from your own runs whether this row meets its done check. Agents in this project have reported passing gates over code that did not compile, so trust nothing below that you have not rerun.
${rowText(row)}
${rules(row)}
Writer summary: ${s.write.summary}. Writer's known gaps: ${s.write.known_gaps.join('; ') || 'none'}
Tests added: ${[...s.verify.tests, ...s.breakr.tests].map(t => t.command).join(' | ')}
Remaining failures after fixing: ${s.remaining.map(f => f.test).join(', ') || 'none'}
Disputed tests: ${disputes.map(d => `${d.test} (${d.reason})`).join('; ') || 'none'}
Run: git -C ${row.worktree} diff master --stat and read the diff. Then cargo fmt --all --check, cargo clippy -p <crate> --all-targets -- -D warnings per touched crate, comment-lint on the touched crates/<crate> and docs paths, cargo test -p <crate> per touched crate, and every command in tests added.${P.godot ? ' Run the Godot acceptance script yourself and copy its RESULT line into godot_result.' : ' Set godot_result to "n/a".'}
For each disputed test, read its cited source and rule test_correct or test_wrong.
Write your reasoning into analysis first. verdict is done only if every clause of the done check is exercised by a passing test you ran, the gate steps pass, and no test ruled correct still fails. Use partial when some clauses are met, failed when the change does not work. evidence is one line in the BACKLOG.csv style: what ran and the counts.`
}

const labelOf = (stage, row) => `${stage}:${row.id}`

const results = await pipeline(
  A.rows,
  row =>
    agent(scoutPrompt(row), { label: labelOf('scout', row), phase: 'Scout', model: 'haiku', effort: 'low', schema: SCOUT }),
  (scout, row) => {
    if (!scout || !scout.feasible) return { row, stopped: 'scout', scout }
    return agent(writerPrompt(row, scout), {
      label: labelOf('write', row),
      phase: 'Write',
      model: writerModel(row),
      effort: writerEffort(row),
      schema: WRITE,
    }).then(write => (write && write.build_ok ? { row, scout, write } : { row, stopped: 'write', scout, write }))
  },
  async s => {
    if (s.stopped) return s
    const [verify, breakr] = await parallel([
      () => agent(verifierPrompt(s.row, s.scout, s.write), {
        label: labelOf('verify', s.row), phase: 'Check', model: 'sonnet', effort: 'medium', schema: VERIFY,
      }),
      () => agent(breakerPrompt(s.row, s.scout, s.write), {
        label: labelOf('break', s.row), phase: 'Check', model: 'sonnet', effort: breakerEffort(s.row), schema: BREAK,
      }),
    ])
    if (!verify || !breakr) return { ...s, stopped: 'check', verify, breakr }
    return { ...s, verify, breakr }
  },
  async s => {
    if (s.stopped) return s
    const commands = [...s.verify.tests, ...s.breakr.tests].map(t => t.command)
    let failing = [
      ...s.verify.failing,
      ...s.breakr.bugs.map(b => ({ test: b.test, failure: `expected ${b.expected}, observed ${b.observed} (${b.source})` })),
    ]
    const fixes = []
    for (let round = 1; round <= 2 && failing.length > 0; round++) {
      const fix = await agent(fixerPrompt(s.row, s.scout, s.write, failing, round), {
        label: labelOf(`fix${round}`, s.row),
        phase: 'Fix',
        model: round === 2 || s.row.difficulty === 'very_high' ? 'opus' : 'sonnet',
        effort: 'high',
        schema: FIX,
      })
      const recheck = await agent(recheckPrompt(s.row, commands), {
        label: labelOf(`recheck${round}`, s.row), phase: 'Fix', model: 'haiku', effort: 'low', schema: RECHECK,
      })
      fixes.push({ round, fix, recheck })
      if (!recheck) break
      const disputed = new Set((fix ? fix.disputed : []).map(d => d.test))
      failing = recheck.failing.filter(f => !disputed.has(f.test))
      log(`${s.row.id} fix round ${round}: ${recheck.failing.length} failing, ${disputed.size} disputed`)
    }
    return { ...s, fixes, remaining: failing }
  },
  async s => {
    if (s.stopped) return s
    const judge = await agent(judgePrompt(s.row, s), {
      label: labelOf('judge', s.row), phase: 'Judge', model: 'sonnet', effort: 'high', schema: JUDGE,
    })
    return { ...s, judge }
  },
)

return results.map((s, i) => {
  const row = A.rows[i]
  if (!s) return { id: row.id, worktree: row.worktree, verdict: 'error', note: 'pipeline stage threw' }
  if (s.stopped) {
    return {
      id: row.id,
      worktree: row.worktree,
      verdict: `stopped at ${s.stopped}`,
      blocker: s.scout ? s.scout.blocker : '',
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
    open_issues: s.judge ? s.judge.open_issues : [],
    disputes: s.judge ? s.judge.disputes : [],
    bugs_found: s.breakr.bugs.length,
    fix_rounds: s.fixes.length,
    files_changed: s.write.files_changed,
  }
})
