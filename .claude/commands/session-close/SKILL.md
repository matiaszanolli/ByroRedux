---
description: "End-of-session ritual — diff stated facts against ground truth and propose synchronised edits across ROADMAP.md / HISTORY.md / README.md"
argument-hint: "[--since <HISTORY section commit>]"
---

# Session Close

Run me at the end of each working session. I check the three durable
project documents against reality, surface drift, and propose a single
synchronised edit.

**Scope**: one session's worth of work. Not a full-project audit — use
`/audit-incremental` for that.

**Prime directive**: each fact lives in exactly one home.
- Bench numbers, test counts, LOC, compat matrix, active milestones → **ROADMAP.md**
- Session narratives → **HISTORY.md**
- Run commands, entry point, "what is this" → **README.md** (links to ROADMAP/HISTORY for the rest)

If you catch me proposing an edit that duplicates a fact across files,
reject it and tell me to link instead.

---

## Step 1 — Resolve the session boundary

Determine the commit range for this session:

```bash
# Find the commit that introduced the most recent HISTORY.md session header
git log --oneline -- HISTORY.md | head -5
git log -1 --format="%H" HISTORY.md
```

If `--since <commit>` was passed, use that. Otherwise use the commit
where the last HISTORY session was appended. If HISTORY has no entries
yet (empty seed), use the oldest unmerged commit on the current branch.

Report the range:

```
Session boundary: <last-history-commit>..HEAD  (<N> commits)
```

If N == 0, exit early — nothing to record.

---

## Step 2 — Gather ground truth

Run these in parallel (single message, multiple Bash calls):

```bash
# Test count — warm compile first, then count
cargo test --workspace --no-run 2>&1 | tail -3
cargo test --workspace 2>&1 | grep -E "^test result:" | \
    awk '{s+=$4} END {print "Total tests passing:", s}'

# Source LOC (non-test and total)
find . -name "*.rs" -not -path "*/target/*" -not -path "*/tests/*" | \
    grep -v "/tests/" | xargs wc -l | tail -1
find . -name "*.rs" -not -path "*/target/*" | xargs wc -l | tail -1

# File + workspace counts
find . -name "*.rs" -not -path "*/target/*" -not -path "*/tests/*" | \
    grep -v "/tests/" | wc -l
grep -c '^\s*"' Cargo.toml || true

# Open issue dirs
ls .claude/issues/ 2>/dev/null | wc -l

# Latest commit
git log -1 --format="%h %s (%ci)"

# Audit-skill drift — stale paths (fatal) + stale symbols (advisory).
# The skills describe the code; a session that moved code moved them
# out of sync. Cheap to run, and the class of error it catches (a GPU
# struct documented at the wrong byte size) is expensive to hit later.
.claude/commands/_audit-validate.sh

# Fix -> issue citation audit for this session's range (#3218). The CI
# traceability gate is `if: github.event_name == 'pull_request'`, and this
# repo's history is overwhelmingly direct commits to main — so for the
# dominant workflow it never fires, and the gap is real: 43 of 134 issues
# closed in the 2026-08-16..20 window (32%) had no commit citing them, 14 had
# no citation anywhere. All 14 were genuinely fixed; what broke was the link
# `/audit-regression` structurally depends on.
scripts/check-issue-traceability.sh --window <session-start-sha> HEAD

# Fixed-but-never-closed audit (#3425) — the direction `--window` cannot
# see, since a fix that lands without ever closing its issue is in neither
# the PR-declared set nor the closed set. Looks for `#NNNN` a commit in
# this session's range added to a `.rs` comment, still OPEN, uncited.
scripts/check-issue-traceability.sh --orphan <session-start-sha> HEAD
```

Fill in the **Ground truth** block below:

```
Ground truth (HEAD = <short-sha>):
  Tests passing:        <N>
  Rust LOC (non-test):  <N>
  Rust LOC (total):     <N>
  Source files:         <N>
  Workspace members:    <N>
  Open issue dirs:      <N>
  Audit-skill drift:    <clean | N stale paths, M advisory symbols>
```

If the audit gate reports anything, fix the skills as part of this
close — do not defer it. Skill drift compounds silently: every later
audit reasons from the stale description and reports findings against
code that no longer exists.

If the citation audit reports a zero-citation set, resolve it **now**,
while you still remember why each issue closed. A closed issue with no
citing commit is indistinguishable, to the next regression sweep, from a
fix that was never made — and the likelier outcome is not a cautious
UNVERIFIABLE but a FAIL filed against working code. Two conventions,
both cheap:

- **Closed by a commit that forgot the keyword** — leave a GitHub close
  comment naming the commit.
- **Closed as a side effect of another issue's fix** (#3102 via #3036,
  #3095's siblings via #2986) — leave a close comment saying
  `resolved as a side effect of #NNNN`. One line is enough, and it is
  the only place that archaeology can live, since by definition no
  commit will ever name it.

If the orphan audit reports a candidate set, check each one before ending
the session — this direction is worse than a missing citation, since the
issue stays OPEN and risks being re-planned or re-implemented later. Two
outcomes, both fine:

- **A forward-looking reference** (a comment naming a future issue this
  session didn't fix, e.g. #3307/#3308) — no action; this is why the mode
  is advisory, not a hard gate.
- **Genuinely fixed here without a closing keyword** — close it with
  `gh issue close <N> --comment "Fixed in <commit-hash>"` before moving on.

Note the sibling hazard recorded in project memory: `Fix #A #B #C`
auto-closes only `#A`. Repeat the keyword per issue — `Fix #A, Fix #B` —
or the bare-ref siblings sit open despite a landed fix. This finding is
the opposite failure of the same discipline: there the keyword is present
but under-applied, here it is absent entirely.

---

## Step 3 — Diff against ROADMAP claims

Read ROADMAP.md → Project Stats table. For each row, compare against
ground truth. Also sweep for bench staleness.

**Bench staleness check.** Find every line in ROADMAP that mentions a
specific FPS / ms / bench commit:

```bash
grep -nE "FPS|ms|commit [0-9a-f]{7}" ROADMAP.md | head -20
```

For each cited commit hash, compute distance from HEAD:

```bash
git rev-list --count <commit>..HEAD
```

Flag any bench older than **30 commits** as stale (needs re-run).

**Repro-command integrity check.** Every claim of the form `X FPS` or
`Y ms` must appear in ROADMAP's "Repro commands for every bench claim"
table. If a claim in the prose lacks a repro row, either:

1. Add the repro command to the table, or
2. Remove/soften the claim.

No exceptions.

Report:

```
Drift detected:
  ROADMAP says tests=924, actual=<N>  (delta +<N>)
  ROADMAP says LOC ~91 300, actual=<N>  (delta +<N>)
  Bench-of-record bee6d48 is <N> commits stale → propose re-run or flag in Known Issues
  <any other drift>

Repro-command integrity:
  <list of claims without repro commands, if any>
```

---

## Step 4 — Draft HISTORY.md append

Walk the session commits:

```bash
git log --oneline <last-history-commit>..HEAD
```

Group commits by theme (NIF parser, ESM parser, renderer, ECS, etc.).
Produce an entry matching the canonical shape in HISTORY.md's header:

```markdown
## Session N — <one-line theme>  (YYYY-MM-DD, <commit range>)

<one-paragraph "why this session happened — what was the driver?">

- **Bucket A** — concrete shipped work with issue refs (`#NNN`)
- **Bucket B** — …
- **Bucket C** — …

Net: <test count delta, LOC delta, any bench delta>
```

**Discipline**:

- One paragraph of context, then buckets. Not a commit log.
- Buckets group by subsystem, not by chronology.
- Every bullet cites its issue (`#NNN`) or commit short-sha.
- "Net" line closes the entry with the numeric delta.
- If the session was a pure bug-bash, call it out: *"audit bundle
  closeout, no milestone churn"* beats *"session ended at commit X."*
- No per-commit noise. The commit log already has that.

Show the draft to the user and ask for edits before writing.

---

## Step 5 — Propose ROADMAP edits

Based on the drift from Step 3 and the session work:

1. **Update Project Stats table** with ground truth.
2. **Update "Last verified"** date at the top.
3. **Update "Bench-of-record"** — if Step 3 flagged staleness, either
   (a) propose a fresh `--bench-frames` run, or (b) add/keep a
   Known Issues line flagging the staleness (R6a-style).
4. **Close completed work** — if a commit range closed an R or M
   milestone, move it from *Active Roadmap* to *Completed Milestones*
   one-liner. Delete the full table row from the active tier.
5. **Add new Known Issues** if the session surfaced them.
6. **Repro-command table hygiene** — add entries for any new bench
   claim the session introduced.

**Do not** append to ROADMAP. Edit in place. If you find yourself
wanting a "Session N retrospective" section in ROADMAP, that belongs
in HISTORY instead.

---

## Step 6 — Propose README edits

README should stay < 120 lines. Only touch it if:

- A headline bench number used in the opening screenshot caption
  changed (re-run from R6a).
- A `cargo run` example is newly broken or newly enabled by the
  session.
- The "State" paragraph is now materially wrong about what works
  today.

Otherwise, leave README alone. It's supposed to be stable.

---

## Step 7 — Unified diff

Show the user all three diffs back-to-back:

```
============================================================
 HISTORY.md  (+<N> lines appended)
============================================================
<diff>

============================================================
 ROADMAP.md  (<N> edits)
============================================================
<diff>

============================================================
 README.md  (<N> edits, or "no changes")
============================================================
<diff>
```

Ask: **"Accept all, edit, or reject?"**

On accept, apply all three edits in one commit. Commit message shape:

```
docs: session N closeout — <one-line theme>

- HISTORY.md: session N narrative appended
- ROADMAP.md: stats refreshed (tests +N, LOC +N), <milestones moved/closed>
- README.md: <one-line description or "untouched">

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
```

---

## Failure modes to avoid

- **Don't auto-apply edits without showing the diff.** This is the
  whole reason it's a ritual and not a hook.
- **Don't write a commit log as the HISTORY entry.** HISTORY is the
  narrative layer on top of git; if it reads like `git log --oneline`,
  it has no reason to exist.
- **Don't duplicate facts across files.** If ROADMAP and README both
  want to state the FPS, README cites "see ROADMAP Status".
- **Don't grow ROADMAP past ~500 lines.** If you need more, something
  should have moved to HISTORY or a `docs/` page.
- **Don't add a bench claim without a repro command.** Every FPS / ms
  number in ROADMAP must have a row in the Repro commands table.
- **If HEAD has uncommitted changes**, run the checks but make the
  unified-diff step advisory only (don't commit until work is staged).
