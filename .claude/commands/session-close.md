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
```

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
