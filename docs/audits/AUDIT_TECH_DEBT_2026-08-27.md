# Tech-Debt Audit — 2026-08-27

**Depth**: deep · **Dimensions**: all 9 · **Sweep**: part of a
`/audit-suite --preset comprehensive` run · **Delta**: 152 commits since
`AUDIT_TECH_DEBT_2026-08-24.md`.

## Scope

Whole workspace (25 crates + `byroredux/`) plus the audit infrastructure
itself (`.claude/commands/`, `_audit-validate.sh`, `scripts/check-issue-
traceability.sh`), which `_audit-common.md` places in scope. Executed by one
agent directly — no sub-agent fan-out, per the dispatch's explicit constraint
(nested-agent relay is unreliable in this project). All analysis is static:
`grep`, `git log -S`/`git blame`/`git grep <rev>`, `gh issue`, the validate
gate, and the SKILL's own `prod_loc` helper. No cargo run.

**Un-owned subsystems touched** (per `_audit-common.md`'s coverage table):
`crates/sdk` (new 2026-08-25, read in full), the gameplay slice, the debug
server. **Not** reached this sweep: `crates/facegen`, `crates/mod-runtime`,
`crates/hkx`, `crates/fsr3-sys` — of which only `crates/facegen` changed in
this window, making it the one omission that could hide *new* debt rather than
merely standing debt (see Deferred).

**Deconflicted**: the concurrent audits in this run had already filed the
GPU-struct doc drift in `docs/engine/shader-pipeline.md` / `memory-budget.md`,
the `docs/feature-matrix.md` Skyrim row, the `audit-character` /
`nifal` / `ui.md` count drifts, and the `commands/physics.rs` layout gap. None
of those is re-filed here. TD4-2026-08-27-04 below covers a **different file
set** (two audit SKILL files, a different struct, a different growth event) and
is not a duplicate.

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 7 |
| **Total** | **9** |

Per-dimension yield:

| Dim | Area | Findings |
|---|---|---|
| 1 | File / Function / Module Complexity | **1** (LOW) |
| 2 | Logic Duplication | **0 — CLEAN** |
| 3 | Stale Documentation & Comments | **1** (MEDIUM) |
| 4 | Audit-Finding Rot | **5** (1 MEDIUM, 4 LOW) |
| 5 | Stale Markers (TODO/FIXME/HACK/XXX) | **0 — CLEAN** (20 hits, all documented exclusion classes, composition unchanged since 2026-08-16) |
| 6 | Stub & Placeholder Implementations | **0 — CLEAN** (`unimplemented!`/`todo!()`/`panic!("not ` still 0 workspace-wide) |
| 7 | Magic Numbers & Hardcoded Constants | **0 — CLEAN** (every shader `#define` outside the generated header is a single alias macro — see Verified Clean) |
| 8 | Dead Code & Backwards-Compat Cruft | **1** (LOW) |
| 9 | Test Hygiene | **1** (LOW) |

**Headline.** The project's fix→issue link is broken in the direction nothing
checks. `scripts/check-issue-traceability.sh` (added 2026-08-24 as #3218's fix)
verifies *closed issue → citing commit*. The failure mode actually occurring
today is the inverse: **a complete fix lands, the author writes the issue
number into the source comment, the commit message omits the closing keyword,
and the issue stays OPEN forever.** Five issues in the current window
(#3149, #3151, #3155, #3244, #3270) are named by a literal `#NNNN` inside
tracked `.rs` source *describing their own fix*, and are still OPEN. Their
fixes landed in two omnibus squash commits — `4e1afcbe` and `98eea9b3` —
neither of which cites a single issue. `4e1afcbe` is itself the commit that
added `check-issue-traceability.sh`. Across the window, **133 of 256
Rust-touching commits carry no closing keyword at all**. This is what caused a
concurrent audit in this very run to spend its budget re-deriving seven
already-fixed defects; it is the highest-value item in this report.

Second: the AI-package cross-cutting trace doc
(`docs/engine/npc-spawn-ai-packages.md`) is written entirely against an API
that no longer exists, and the validate gate cannot see it because
`should_skip` discards every backticked bare basename — including the ten
citations of `` `ai.rs` ``, a file deleted under #2054.

The rest is quiet. Markers, stubs and shader-constant provenance are clean and
unchanged. Duplication spot-checks over the largest deltas (`mesh.rs` +875,
`cinematic.rs` +749, `crates/sdk` in full) found no reimplemented logic.

### Premises investigated and not sustained

- *"`#[ignore]` collapsed 171 → 126 — 45 tests were deleted."* The 08-24
  baseline is wrong. Re-measured at that report's own HEAD (`07a029ea`, the
  last commit of 2026-08-24) with its own stated recipe: **121**, not 171. No
  variant reproduces 171 (anchored-with-reason: 149; unanchored `.rs`: 200;
  whole repo: 550). The true movement is 121 → 126, ordinary growth. Filed as
  TD4-2026-08-27-03 because that number is in a section explicitly labelled
  "for the next audit's diff".
- *"#3237 (GRUP recursion depth) is CLOSED while its bug is live."* Reported
  by a concurrent audit; not sustained from this audit's own reading. Every
  recursive GRUP walker found — `records/grup_walker.rs` (four `_inner`
  recursions), `cell/walkers.rs:140`, `cell/wrld.rs:274,289` — threads a
  `depth` counter through `reader.bounded_group_content_end`, gated on
  `MAX_GRUP_NESTING_DEPTH = 64` (`esm/reader.rs:32,774`). Not re-filed and not
  contradicted here — the concurrent audit owns that dimension and may have
  found a path this sweep did not enumerate; recorded so the two reports are
  not read as agreeing.
- *"`crates/sdk` (new, un-owned) duplicates existing bounds/AABB math."*
  `AssetBounds::from_spheres` (`crates/sdk/src/studio.rs:20-48`) has no
  counterpart in `crates/core` or `crates/renderer` — the only sibling types
  are `FogBounds` and `PlumeBounds`, both semantically different. 282 LOC
  across two files, one dependency (`byroredux-core`). Clean.
- *"`crates/renderer/src/mesh.rs`'s +875-line growth is duplicated scaffolding."*
  It is `#3298`'s chunked geometry-SSBO rebuild plus `#3372`'s compaction gate.
  Largest function is 167 LOC; the resumable path documents in prose why it is
  not the atomic path, and the atomic path survives as the named low-headroom
  fallback. Legitimate production growth — filed under Dim 1 (file size) only.
- *"`byroredux/src/components.rs`'s ten `#[allow(dead_code)]` sites are a new
  cluster."* Eight are documented staged-rollout or protocol-capture markers
  the SKILL excludes. Only the `PersistentRefIndex` pair has a justification
  that has since gone stale — filed as TD8-2026-08-27-01.

---

## Baseline Snapshot (for the next audit's diff)

Measured at HEAD `969d81c8` with the SKILL's Phase-1 recipes verbatim.

```
TODO/FIXME/HACK/XXX:          20   (unchanged; 0 real — all ESM XXXX protocol,
                                    upstream-FIXME references, or prose)
allow(dead_code):             69   (was 68 at the 08-24 HEAD; +1)
unimplemented!/todo!():        0   (unchanged)
#[ignore] tests (SKILL recipe, *.rs): 126   (was 121 at the 08-24 HEAD —
                                    NOT 171; see TD4-2026-08-27-03)
#[ignore] incl. `= "reason"` form:    155   (the recipe misses 29; TD9-2026-08-27-01)
files >2000 production LOC:    5   (was 4; `crates/renderer/src/mesh.rs` newly crossed)
files >2000 total LOC:        27   (was 19)
```

Dim-1 **production** bucket — re-verified with `prod_loc`:

| Prod LOC | Total LOC | File | Issue | Delta vs 08-24 |
|---|---|---|---|---|
| 3607 | 4947 | `crates/renderer/src/vulkan/context/draw.rs` | **#3282 (OPEN)** — `draw_frame` regrowth | +27 prod |
| 2859 | 3745 | `crates/renderer/src/vulkan/volumetrics.rs` | **#2256 (OPEN)** | unchanged |
| 2401 | 2878 | `crates/renderer/src/vulkan/context/mod.rs` | none open (#1749 closed) | **+95 prod** — regrowing after the `init.rs`/`teardown.rs` split |
| 2049 | 2896 | `crates/renderer/src/mesh.rs` | none — **TD1-2026-08-27-01** | **+524 prod** (was 1525) |
| 2013 | 2021 | `crates/renderer/src/texture_registry.rs` | #2977 (CLOSED, recorded) | unchanged |

Secondary (total-LOC) bucket grew 19 → 27 members. Every new entry checked
with `prod_loc`; none besides `mesh.rs` crosses into the primary bucket. New
this window and worth watching (prod LOC in parens):
`byroredux/src/boot.rs` (2047 total), `byroredux/src/material_translate.rs`
(2047 total), `crates/physics/src/water.rs` (2182 total),
`crates/renderer/src/vulkan/buffer.rs` (2112 total),
`crates/core/src/ecs/components/material.rs` (2194 total).

---

## Top Quick Wins (trivial, ≤30 min each)

1. **Close the seven orphaned issues** (#3149, #3151, #3155, #3244, #3270, and
   the two the concurrent run named) with a comment naming the commit that
   fixed them. Immediate, and it stops the next audit repeating this run's
   wasted effort. (TD4-2026-08-27-01)
2. **TD4-2026-08-27-04** — update `GpuCamera` 352 → 368 B and
   `gpu_camera_is_352_bytes` → `gpu_camera_is_368_bytes` at
   `.claude/commands/audit-renderer/SKILL.md:115` and
   `.claude/commands/audit-regression/SKILL.md:149`. Both are already on the
   gate's own advisory list; clearing them is one `sed`.
3. **TD4-2026-08-27-03** — correct the `#[ignore]` figure in
   `docs/audits/AUDIT_TECH_DEBT_2026-08-24.md`'s Baseline Snapshot (171 → 121)
   so the next diff is not read as a 45-test deletion.
4. **TD4-2026-08-27-02** — one-line change in `_audit-validate.sh:44`: only
   skip a bare basename when it resolves as a path suffix; report it when it
   resolves nowhere. Recovers the ten dead `` `ai.rs` `` refs the gate is
   currently blind to.

## Top Medium Investments

1. **TD4-2026-08-27-01 — add the inverse traceability check.** `--window` mode
   already walks `base..head`; the missing half is: for every `#NNNN` appearing
   in a `.rs` file changed in the window, if that issue is OPEN and no commit in
   the window cites it with a closing keyword, report it. That single addition
   would have caught all five orphans mechanically. Pairs with a commit-hygiene
   rule against omnibus squashes (`4e1afcbe` bundles nine unrelated
   sub-messages across 39 files; `3aebf414`'s message describes only a smoke-test
   fixture refactor while the diff also deletes fifteen `pack.rs` functions).
2. **TD3-2026-08-27-01 — rewrite `docs/engine/npc-spawn-ai-packages.md`'s API
   sections** against the live `active_package` + `PackRecord::is_*` pair. Best
   done in the same pass as #3351, which owns a *different* stale claim class
   in the same file (selection semantics and pathing, lines 222-224/452-454/
   473-476, disjoint from this finding's lines).
3. **Carry-over, still the two widest OPEN Dim-1 items**: #3282 (`draw_frame`,
   now 51%+ of a 4947-line file) and #2256 (`volumetrics.rs`). `context/mod.rs`
   is quietly regrowing (+95 prod LOC in four days) after its #1749 split —
   the third occurrence of the regrowth pattern this file family keeps
   producing.

---

# Findings

## MEDIUM

### TD4-2026-08-27-01: the fix→issue link is only checked in one direction — a fix that lands without a closing keyword leaves its issue OPEN forever, and five current issues are named by their own fix's source comment

- **Severity**: MEDIUM
- **Dimension**: 4 — Audit-Finding Rot (promotion trigger: *"Stale doc/audit
  baseline that misled an audit in the last 90 days"* — it misled a concurrent
  audit in **this** run)
- **Location**: `scripts/check-issue-traceability.sh:36-104` (`--window` mode);
  the orphaned fixes at `crates/ui/src/avm2_host.rs:224`, `:1521`,
  `byroredux/src/main.rs:119`,
  `crates/renderer/src/vulkan/morph_compute.rs:45,187,219`,
  `crates/plugin/src/esm/records/misc/water.rs:1068-1072`
- **Status**: NEW (**not** a re-file of #3218, which is CLOSED and fixed the
  *opposite* direction — see Description)
- **Age**: the script landed `4e1afcbe` (2026-08-24); the orphans it cannot see
  landed in `4e1afcbe` and `98eea9b3` (2026-08-25)
- **Effort**: small (the check) + medium (the commit-hygiene half)
- **Description**: #3218 established that 43 of 134 issues closed in the
  2026-08-16..20 window had no citing commit, and its fix added `--window` mode
  to `check-issue-traceability.sh`. That mode enumerates issues **already
  closed** in the window and asks whether a commit cites each one. Both of the
  script's modes take the closed/declared set as their input:

  ```bash
  # PR mode — input is the PR body's declared closes
  mapfile -t closing_issues < <(printf '%s\n' "${pr_body}" | closing_issue_numbers)
  # --window mode — input is gh's closed-issue list
  gh issue list --state closed --limit 500 --search "closed:>=${since%T*}" ...
  ```

  An issue that was **fixed but never closed** is in neither input. It is not in
  the PR body (the script's own comment records that "this repo's history is
  overwhelmingly direct commits to main, so for the dominant workflow that gate
  never fires at all"), and by definition it is not in the closed set. It is
  therefore structurally invisible to the tool built to protect this exact
  linkage — and unlike the direction #3218 covered, this one does not merely
  lose archaeology: **the issue stays open, so the work is re-planned, re-audited,
  and can be re-implemented or reverted.**

  The signal that makes it mechanically detectable is already present in the
  tree: the fix author writes the issue number into the source comment. Five
  currently-OPEN issues are named by a literal `#NNNN` inside tracked `.rs`
  source that describes their own fix.
- **Evidence**:
  ```
  # Every `#NNNN` appearing in tracked .rs source, intersected with OPEN issues:
  $ grep -rhoE '#[0-9]{4}' --include='*.rs' crates byroredux | sort -nu   # 1489 distinct
  #   OPEN today: 3149 3151 3155 3244 3270 3307 3308
  #   (3307/3308 are legitimate — their commits say "Document #3307's technical
  #    blocker" and "Partial #3308". The other five are complete fixes.)

  $ grep -rn "#3270" crates/plugin/src/esm/records/misc/water.rs
  1068:    // FO4's first float is the depth amount. Offsets 12/16 are not fog
  1069:    // distances: across vanilla Fallout4.esm they are normalized values near
  1070:    // 1.0, and treating them as distances collapses every ramp to ~1 BU
  1071:    // (#3270). Keep the canonical 80/600 fog defaults until those fields'
  1072:    // actual shader roles are identified.

  $ git log --format="%h %ad %s" --date=short -1 -S "Offsets 12/16 are not fog" \
        -- crates/plugin/src/esm/records/misc/water.rs
  98eea9b3 2026-08-25 Refactor exterior session reload and bootstrap mode handling

  $ gh issue view 3270 --json state -q .state
  OPEN
  ```
  The commit whose *code* names #3270 has a subject about exterior session
  reload and no issue reference anywhere in its body. The same holds for the
  other four:

  | Issue | Fix site (verified present) | Landing commit | Commit cites it? |
  |---|---|---|---|
  | #3149 | `crates/ui/src/avm2_host.rs:224` | `4e1afcbe` | no |
  | #3151 | `crates/ui/src/avm2_host.rs:1521` | `4e1afcbe` | no |
  | #3155 | `byroredux/src/main.rs:119` | `4e1afcbe` | no |
  | #3156 | `crates/ui/src/navigator.rs:95-114` (`MAX_IMPORT_ASSET_PATHS` cap + `import_asset_paths_capped` latch + a passing test at `:629-636`) | `4e1afcbe` | no |
  | #3191 | `byroredux/src/systems/billboard.rs:233-239` (the exact pre-multiplication the issue asks for, plus `reversing_wind_reverses_mean_lean`) | `4e1afcbe` | no |
  | #3244 | `crates/renderer/src/vulkan/morph_compute.rs:45,187,219` + `context/draw.rs:1507` | `98eea9b3` | no |

  `4e1afcbe`'s full body is nine Conventional-Commits sub-lines across 39 files
  and 927 insertions, with zero `#NNNN`:
  ```
  refactor: unify actor value key space and improve documentation
  fix: correct NUL byte handling in bone name strings
  ... (7 more) ...
  scripts: add checks for issue traceability and source integrity
  ```
  — i.e. the commit that introduced the traceability tool is itself the single
  largest violation of the practice the tool exists to enforce.

  Window-wide: **133 of 256 Rust-touching commits since 2026-08-20 carry no
  closing keyword** (162 of 303 including docs-only commits). The same omnibus
  shape recurs in `3aebf414`, whose message describes only a smoke-test fixture
  refactor while its diff also deletes fifteen `pack.rs` public functions and
  converts them to `#[cfg(test)]` macro shims.
- **Impact**: Two compounding costs, both realised. (1) `/audit-regression`'s
  Step 2.1 is `git log --grep="#<N>"`; for these issues it returns nothing, so
  the audit cannot distinguish "no citation" from "no fix" — the script's own
  comment calls this degradation "self-concealing". (2) More expensive: the
  issues remain OPEN, so they are re-triaged and re-investigated. This run's
  concurrent audits independently rediscovered all seven, spending budget on
  defects fixed days earlier — and a future `/fix-issue` on one of them could
  re-implement or revert live, correct code.
- **Related**: #3218 (CLOSED — the forward direction, whose fix is the script
  this finding extends); the `feedback_multi_issue_commit_close` memory note
  (the older, narrower instance of the same class: `Fix #A #B #C` auto-closes
  only `#A`).
- **Suggested Fix**: Add a third mode to `check-issue-traceability.sh` — for
  every `#NNNN` in the `.rs` diff of `base..head`, if `gh issue view` reports
  OPEN and no commit in the range cites it with a closing keyword, list it as a
  candidate orphan. It is advisory-shaped like `--window` (some code refs are
  legitimately forward-looking, e.g. #3307/#3308) but would have surfaced all
  five here mechanically. Pair it with a commit-hygiene rule: an omnibus squash
  must carry one closing keyword per issue its diff resolves. Then close the
  seven current orphans with a comment naming their landing commit.

---

### TD3-2026-08-27-01: `npc-spawn-ai-packages.md` — a designated-authoritative cross-cutting trace — is written against a deleted API and cites a file that no longer exists, ten times

- **Severity**: MEDIUM
- **Dimension**: 3 — Stale Documentation & Comments
- **Location**: `docs/engine/npc-spawn-ai-packages.md:66, 105, 121, 147-151,
  154-161, 169, 211-212, 250, 316-317, 348-349, 372, 391-392, 445, 466`
- **Status**: NEW (**not** a re-file of #3351, which is OPEN against
  *different lines* of the same file — see Related)
- **Age**: two events. `ai.rs` was split into `records/misc/` under #2054
  (2026-07); the fifteen `active_*` selectors were deleted under #3042 by
  `3aebf414` (2026-08-27, one day before this audit)
- **Effort**: small
- **Description**: `_audit-common.md`'s Key Reference Docs table names this file
  as "Cross-cutting trace #4 … NPC_ spawn → AI package selection → per-procedure
  runtime" and instructs every audit to *"prefer them over re-deriving facts
  from source"*. An auditor or contributor who follows that instruction today is
  handed three separate dead references:

  1. **Ten backticked citations of `` `ai.rs` ``**, four of them with line
     numbers (`ai.rs:20`, `ai.rs:147,159`). No file named `ai.rs` exists
     anywhere in the tree; the content moved to
     `crates/plugin/src/esm/records/misc/pack.rs` (and siblings) under #2054.
  2. **Eight symbols that no longer exist at all**: `active_sandbox_location`,
     `active_wander_location`, `active_travel_location`, `active_follow_target`,
     `active_escort_target`, `active_escort_location`, `active_guard_location`,
     `active_patrol_location`. All eight are already on the validate gate's
     docs/engine symbol advisory.
  3. **Seven names that exist but no longer mean what the doc says**:
     `active_package_is_sandbox` … `active_package_is_patrol` are now
     `macro_rules!`-generated **`#[cfg(test)]`-only shims**
     (`pack.rs:777-794`), not the production selectors the doc describes as
     gating behaviour inserts.

  The doc's central mechanism paragraph is therefore false at every level: the
  described functions are gone, the described file is gone, and the described
  gating no longer happens the way it says.
- **Evidence**:
  ```
  $ ls crates/plugin/src/esm/records/misc/ai.rs
  ls: cannot access '...': No such file or directory
  $ grep -c '`ai\.rs' docs/engine/npc-spawn-ai-packages.md
  10

  $ grep -rn "active_sandbox_location" crates byroredux --include='*.rs'
  (no output)

  $ sed -n '763,790p' crates/plugin/src/esm/records/misc/pack.rs
  #[cfg(test)]
  mod tests {
      ...
      /// #3042 — the seven production `active_package_is_*` wrappers were
      /// deleted as dead code (#2031 collapsed the spawn tail onto a single
      /// `active_package` resolve and left them unreachable). ...
      macro_rules! active_package_is {
      ...
      active_package_is!(active_package_is_sandbox, is_sandbox);
  ```
  And the doc, unchanged:
  ```
  docs/engine/npc-spawn-ai-packages.md:169
  `active_package_is_sandbox`/`active_sandbox_location` (`ai.rs:147,159`)
  feed `npc_spawn.rs`, which inserts `SandboxBehavior { search_radius }`
  ```
- **Impact**: Documentation-only, but on the tier audits are told to trust. A
  reader grepping for any of the eighteen named symbols/paths gets nothing and
  must reverse-engineer the live path (`active_package` + `PackRecord::is_*`,
  as `_audit-common.md`'s Sandbox AI row correctly describes) from scratch. Two
  of this file's rot classes are now tracked independently (#3351 and this
  finding), which is itself a signal the file needs one consolidating pass
  rather than three point edits.
- **Related**: #3351 (OPEN — same file, disjoint lines 222-224/452-454/473-476,
  claims about spawn-time-only selection and no-pathing; fix both together).
  #3042 (CLOSED — deleted the code; the doc was not updated with it). #2054
  (CLOSED — the `ai.rs` split). TD4-2026-08-27-02 below (why the gate did not
  catch the `ai.rs` half).
- **Suggested Fix**: Rewrite §4/§5 and the six per-procedure sections against
  `active_package` + `PackRecord::is_*`, replacing every `` `ai.rs` `` with the
  live `crates/plugin/src/esm/records/misc/pack.rs`; italicise the eight deleted
  getter names as historical per the path-reference convention, or drop them.
  Land alongside #3351.

---

## LOW

### TD4-2026-08-27-02: `_audit-validate.sh` skips every backticked bare basename — a deleted file is invisible to the gate, which is why ten dead `ai.rs` refs pass

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-validate.sh:38-44` (`should_skip`)
- **Status**: NEW
- **Effort**: trivial
- **Description**: The gate's first skip rule discards any reference without a
  `/`:
  ```bash
  should_skip() {
      local p="$1"
      # Bare basenames (`lib.rs`, `systems.rs`, `tests.rs`) are used as
      # shorthand inside a paragraph that already established the dir
      # context. They carry no path info to begin with, so they can't
      # go stale in the "wrong dir" sense this gate targets.
      [[ "$p" != */* ]] && return 0
  ```
  The stated rationale is sound for its stated case and wrong for the case that
  actually occurred. A bare basename carries no *directory* information, but it
  still asserts **existence** — and the machinery to check exactly that is
  already in the file two functions later:
  ```bash
  path_exists() {
      local p="$1"
      [[ -e "$p" ]] && return 0
      grep -qE "(^|/)${p//./\\.}\$" "$all_paths_file"   # path-suffix match
  }
  ```
  `path_exists "ai.rs"` returns false today — `git ls-files | grep -E '(^|/)ai\.rs$'`
  is empty — so the gate has everything it needs and is prevented from using it
  by the skip. #3202 extended the gate to `docs/engine/*.md` precisely so
  reference docs get "the same policing as the skills"; this blind spot means
  ten citations in one such doc still pass silently.
- **Evidence**:
  ```
  $ .claude/commands/_audit-validate.sh | tail -1
  OK: all path references valid.

  $ git ls-files | grep -E "(^|/)ai\.rs$" ; echo "exit=$?"
  exit=1

  $ grep -c '`ai\.rs' docs/engine/npc-spawn-ai-packages.md
  10
  ```
- **Impact**: One structural class of doc rot — *the file was deleted, not
  moved* — is unreachable by the gate, in exactly the tier #3202 added to close
  that hole. Low blast radius (documentation only) but it defeats the gate's
  purpose on its newest and least-reviewed input set.
- **Related**: #3202 (CLOSED — extended the glob to `docs/engine/`, the change
  this finding completes); #3197 (CLOSED — two earlier gate blind spots);
  TD3-2026-08-27-01 (the rot this blind spot hid).
- **Suggested Fix**: Replace the unconditional skip with a conditional one —
  skip a bare basename only when `path_exists` succeeds (it is genuinely
  shorthand); report it when it resolves nowhere in the tree. Two lines. Expect
  a small first-run advisory backlog of legitimately-generic names
  (`lib.rs`, `mod.rs`, `tests.rs` all resolve, so they stay silent).

---

### TD4-2026-08-27-03: `AUDIT_TECH_DEBT_2026-08-24.md`'s `#[ignore]` baseline is 171 where the real figure was 121 — the section is explicitly labelled "for the next audit's diff"

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `docs/audits/AUDIT_TECH_DEBT_2026-08-24.md` — the Baseline
  Snapshot block, line reading `#[ignore] tests (*.rs only): 171`; repeated in
  that report's Executive Summary Dim-9 row and its "Premises investigated"
  section
- **Status**: NEW
- **Effort**: trivial
- **Description**: The 08-24 report's Dim-9 narrative rests on the figure 171
  ("was 154; the bare `.` recipe over the whole tree reads 503 — 313 are
  docs/markdown false hits"). The 503 and 313 figures are reproducible; 171 is
  not. Measured at that report's own HEAD (`07a029ea`, the last commit of
  2026-08-24) with the recipe the SKILL prescribes and the report says it used,
  the count is **121**. No variant of the recipe yields 171.
- **Evidence**:
  ```
  $ git grep -h -E '^[[:space:]]*#\[ignore\]' 07a029ea -- '*.rs' | wc -l
  121                                   # the SKILL's recipe, .rs only
  $ git grep -h -E '^[[:space:]]*#\[ignore'  07a029ea -- '*.rs' | wc -l
  149                                   # + the `= "reason"` form
  $ git grep -h -E '#\[ignore'          07a029ea -- '*.rs' | wc -l
  200                                   # unanchored
  $ git grep -h -E '#\[ignore'          07a029ea             | wc -l
  550                                   # whole repo (the report's own 503-class figure)

  # Same commit, the other three baselines, for contrast — these reproduce:
  markers: 20 (report: 20)   allow(dead_code): 68 (report: 69)
  ```
- **Impact**: This audit had to spend a measurement cycle disproving an
  apparent 45-test deletion before it could report a baseline. Left uncorrected
  the next sweep repeats that, or worse files a phantom "test coverage
  regression" finding. This is the failure mode the Baseline Snapshot section
  exists to prevent, occurring in the Baseline Snapshot section.
- **Related**: #2262 (CLOSED — the *other* `#[ignore]`-count recipe defect, a
  false 2.4× regression from whole-repo textual scanning; that one is real and
  the 08-24 report correctly identified it, which makes the un-reproducible
  number sitting beside it easy to miss). TD9-2026-08-27-01 (a third, separate
  defect in the same recipe).
- **Suggested Fix**: Amend the 08-24 report's Baseline Snapshot to `121` with a
  one-line note that the previously-published `171`/`154` pair is
  unreproducible, so the number is corrected at the place the next audit reads
  rather than only in this report.

---

### TD4-2026-08-27-04: two audit SKILL files pin `GpuCamera` at 352 B and name a test that no longer exists — the struct grew to 368 B on 2026-08-26

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/audit-renderer/SKILL.md:115`,
  `.claude/commands/audit-regression/SKILL.md:149`
- **Status**: NEW (**distinct file set** from the `docs/engine/shader-pipeline.md` /
  `memory-budget.md` drift a concurrent audit filed this run — different files,
  different struct, different growth event; not re-filed)
- **Age**: `4dcbd187` (2026-08-26, #3323 — `exterior_sky_tint` vec4 appended)
- **Effort**: trivial
- **Description**: Both skills carry a "sizes pinned by tests — confirm they
  hold" instruction naming `gpu_camera_is_352_bytes`. That test no longer
  exists; `GpuCamera` grew 352 → 368 B when #3323 appended `exterior_sky_tint`,
  and the live pin is `gpu_camera_is_368_bytes`. `audit-renderer/SKILL.md:115`
  is otherwise scrupulously current — its `GpuInstance` (160 B) and
  `GpuMaterial` (432 B, with the full 260→…→432 history) entries were updated
  through 2026-08-25 — so the `GpuCamera` clause is a single missed field in an
  otherwise maintained line, not general neglect.
- **Evidence**:
  ```
  $ grep -rn "fn gpu_camera_is" crates/renderer/src/
  crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:66:fn gpu_camera_is_368_bytes() {
        assert_eq!(size_of::<GpuCamera>(), 368,
            "GpuCamera must be 368 B (352 B + 16 B exterior_sky_tint vec4, #3323) ...");

  $ .claude/commands/_audit-validate.sh | sed -n '4,12p'
  ADVISORY (audit skills) — backticked symbols not found in any tracked source file:
    ...
    gpu_camera_is_352_bytes                        audit-regression audit-renderer
    ...
  ```
  The gate's own advisory already names it — this is the fourth consecutive
  audit in which the GPU-struct size drift recurs, and the first in which the
  gate flagged it and the flag was not acted on.
- **Impact**: An auditor following `audit-regression/SKILL.md:149`'s "Run them:"
  instruction runs a test name that does not exist and gets a silent zero-test
  pass, which reads as green. Blast radius is the audit tier only.
- **Related**: #3201 (the 336→352 instance), #3240, and the
  `shader-pipeline.md`/`memory-budget.md` sites filed concurrently this run.
  This is the *sixth* recurrence of one mechanism — see the note below.
- **Suggested Fix**: `352 B` → `368 B` and `gpu_camera_is_352_bytes` →
  `gpu_camera_is_368_bytes` at both sites, with the growth history appended in
  the style `audit-renderer/SKILL.md` already uses for the other two structs.

  **Mechanism note** (offered rather than filed, since the parent run already
  owns the instance findings): every recurrence has been fixed by hand, and each
  hand fix has held for exactly as long as the struct did not grow again. The
  durable fix is generation, not vigilance — have `crates/renderer/build.rs`,
  which already emits `shaders/include/shader_constants.glsl` from
  `shader_constants_data.rs`, additionally emit a small generated table of
  `struct → size → pin-test-name`, and have `_audit-validate.sh` diff the
  backticked sizes in `.claude/commands/**` and `docs/engine/**` against it.
  That converts a recurring MEDIUM into a build failure at the moment of growth.

---

### TD1-2026-08-27-01: `crates/renderer/src/mesh.rs` newly crossed 2000 production LOC (1525 → 2049 in four days), taking the primary bucket from 4 to 5

- **Severity**: LOW
- **Dimension**: 1 — File / Function / Module Complexity
- **Location**: `crates/renderer/src/mesh.rs` (2049 production / 2896 total LOC)
- **Status**: NEW
- **Age**: `ae7179a3` (#3298 — resumable geometry-SSBO rebuild) and `cd1aa9e9`
  (#3372 — compacted-offset publication gate), 2026-08-26/27
- **Effort**: medium
- **Description**: The file now carries three distinct responsibilities that
  grew independently: (a) `MeshRegistry` proper — handle allocation, per-mesh
  metadata, LRU eviction; (b) the **global geometry SSBO lifecycle**, which is
  where all the new code went: `build_geometry_ssbo` (:1112),
  `rebuild_geometry_ssbo` (:1208), `advance_geometry_rebuild` (:1348),
  `rebuild_geometry_ssbo_atomic_fallback` (:1507), plus the free function
  `next_geometry_rebuild_chunk` (:152) and the generation counter; (c) the
  primitive-geometry helpers (`cube_vertices` and siblings, 167 LOC for the
  largest).

  Unlike the other four primary-bucket members this is **not** a long-function
  problem — the largest function is 167 LOC, well under the 200-LOC extraction
  trigger, and the new chunked-rebuild code is unusually well documented (it
  explains in prose why the atomic path survives as the low-headroom fallback
  rather than being deleted). It is purely a file-cohesion crossing, and (b) is
  a self-contained state machine with its own generation counter, chunk cursor
  and fallback path — a clean extraction seam.
- **Evidence**:
  ```
  $ prod_loc crates/renderer/src/mesh.rs                    # SKILL Phase-1 helper
  2049
  $ git show 07a029ea:crates/renderer/src/mesh.rs > /tmp/old.rs; prod_loc /tmp/old.rs
  1525
  $ grep -n "fn build_geometry_ssbo\|fn rebuild_geometry_ssbo\|fn advance_geometry_rebuild\|fn rebuild_geometry_ssbo_atomic_fallback\|fn next_geometry_rebuild_chunk" crates/renderer/src/mesh.rs
  152:fn next_geometry_rebuild_chunk(
  1112:    pub fn build_geometry_ssbo(
  1208:    pub fn rebuild_geometry_ssbo(
  1348:    fn advance_geometry_rebuild(
  1507:    fn rebuild_geometry_ssbo_atomic_fallback(
  ```
  `gh issue list --search "mesh.rs in:title" --state open` returns nothing —
  the only prior mesh.rs Dim-1-adjacent issue is #1760 (CLOSED, two dead `pub
  fn`).
- **Impact**: Maintenance only. Worth filing now rather than after another
  growth cycle: this file gained +524 production LOC in four days across two
  issues, and the third primary-bucket member (`context/mod.rs`) is
  simultaneously regrowing +95 after its own #1749 split — the bucket is
  trending up, not down.
- **Related**: #3282 (`draw_frame`), #2256 (`volumetrics.rs`) — the two OPEN
  primary-bucket items; #3298/#3372 (the two closed issues whose work landed
  here).
- **Suggested Fix**: Extract the global geometry SSBO lifecycle into
  `crates/renderer/src/mesh/geometry_ssbo.rs` — the five functions above plus
  `geometry_generation`, `ssbo_vertex_count`/`ssbo_index_count`,
  `geometry_dirty`, the rebuild cursor and `geometry_staging_pool` — leaving
  `mesh.rs` as `MeshRegistry` + primitives. Mechanical: the block already
  communicates with the rest of the file through a small, named field set.

---

### TD8-2026-08-27-01: `PersistentRefIndex` is fully dead — inserted at boot, never built, read, or invalidated — and the milestone its `#[allow(dead_code)]` names as the pending consumer closed two days ago

- **Severity**: LOW
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `byroredux/src/components.rs:1340-1358` (struct + two
  field-level allows), `byroredux/src/cell_loader/persistent_ref_index.rs:45,67`
  (two function-level allows), `byroredux/src/boot.rs:496` (the live insertion)
- **Status**: NEW
- **Effort**: trivial (retarget the justification) or small (wire or remove)
- **Description**: The SKILL correctly excludes "landed ahead of its consumer"
  code from Dim 8 — *note it, do not delete it*. This one has passed the point
  where that exclusion applies to its own stated terms. Both the struct doc and
  all four `#[allow(dead_code)]` comments name the same two gating milestones:

  ```rust
  /// Landed ahead of its consumer, same posture as `groundcover_translate`'s
  /// Phase 0 constants: fully exercised by `cell_loader::persistent_ref_index`'s
  /// test suite, a *pending* production consumer (EX-14/15, EX-16) rather than
  /// unused code — hence the field-level `#[allow(dead_code)]` below.
  pub(crate) struct PersistentRefIndex {
      #[allow(dead_code)] // see the struct doc — EX-14/15/EX-16 is the pending consumer
  ```
  **EX-14/15 is #2369, CLOSED 2026-08-26** — it shipped without wiring the
  index. Only EX-16 (#2372) remains open, so the justification is now half
  false at four sites. Meanwhile the resource is inserted into the live `World`
  at `boot.rs:496` and nothing in production ever calls
  `resolve_persistent_ref` or `invalidate`; the only callers are its own tests.
- **Evidence**:
  ```
  $ grep -rn "persistent_ref_index::" byroredux/src --include='*.rs' | grep -v tests
  byroredux/src/cell_loader/cell_root_ref_index.rs:40:/// `persistent_ref_index::invalidate`'s own rationale).   # a doc comment, not a call

  $ grep -rn "PersistentRefIndex" byroredux/src --include='*.rs' | grep -v tests | grep -v components.rs
  byroredux/src/boot.rs:496:    world.insert_resource(crate::components::PersistentRefIndex::new());
  byroredux/src/cell_loader/persistent_ref_index.rs:23:use crate::components::PersistentRefIndex;

  $ gh issue view 2369 --json state,closedAt -q '.state+" "+.closedAt'
  CLOSED 2026-08-26T20:54:20Z
  ```
  The sibling `CellRootRefIndex` (same file, same pattern, same `boot.rs:497`
  insertion) is **not** part of this finding — its named consumer is
  stream-boundary-state-continuity / #3299, which is genuinely still open.
- **Impact**: Negligible at runtime (one empty `HashMap` resource). The real
  cost is that a stale "pending" justification is exactly what turns
  land-ahead-of-consumer code into permanent dead code: the next Dim-8 sweep
  reads the comment, sees a named milestone, and skips it — as every sweep
  since the code landed has done.
- **Related**: #2369 (CLOSED — the milestone that shipped without wiring it),
  #2372 (OPEN — the remaining gate), #3299 (the sibling's live gate).
- **Suggested Fix**: Retarget all four `#[allow(dead_code)]` comments and the
  struct doc to name **only** #2372, so the next reader sees one live gate
  rather than a closed one; and add a line to #2372 recording that
  `resolve_persistent_ref` already exists and is waiting on it. If EX-16 is
  going to reach the index by a different route, delete the resource, the
  module and the `boot.rs` insertion instead — `form_id_root_index::resolve`,
  the shared logic underneath, stays live via `CellRootRefIndex`.

---

### TD9-2026-08-27-01: Dim 9's own discovery recipe misses the `#[ignore = "reason"]` form — 29 tests today, a 19% undercount

- **Severity**: LOW
- **Dimension**: 9 — Test Hygiene
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Phase-1
  snapshot recipe and Dimension 9's Discovery block, both
  `grep -RIn '^\s*#\[ignore\]' --include='*.rs' crates byroredux`
- **Status**: NEW (a third, distinct defect in this recipe — see Related)
- **Effort**: trivial
- **Description**: The pattern requires `#[ignore]` to close immediately after
  the attribute name, so Rust's documented reason form `#[ignore = "…"]` never
  matches. 29 such tests exist today and none of them appears in any tech-debt
  audit's Dim-9 count or triage.
- **Evidence**:
  ```
  $ grep -RIn -E '^[[:space:]]*#\[ignore\]'  --include='*.rs' crates byroredux | wc -l
  126     # the SKILL recipe
  $ grep -RIn -E '^[[:space:]]*#\[ignore'    --include='*.rs' crates byroredux | wc -l
  155     # + the reason form
  $ grep -RIn -E '^[[:space:]]*#\[ignore[[:space:]]*=' --include='*.rs' crates byroredux | wc -l
  29
  ```
  **Triaged — the substance is clean.** All 29 carry an explicit data/GPU gate
  and none guards a closed CRITICAL/HIGH fix:
  ```
  crates/scripting/tests/pex_recognize_e2e.rs:37  #[ignore = "needs Skyrim SE game data on disk"]
  byroredux/tests/skinning_e2e.rs:151             #[ignore = "requires FNV BSA — opt in with --ignored"]
  byroredux/tests/cornell_rt_oracle.rs:26         #[ignore = "requires an RT-capable Vulkan device and a display/Xvfb"]
  byroredux/tests/golden_frames.rs:66             #[ignore = "requires Vulkan device + release build; opt-in via --ignored"]
  ... (25 more, same three classes)
  ```
- **Impact**: No live debt hidden today — but the reason form is the one an
  author reaches for when the reason is *"blocked on #NNNN"*, which is exactly
  the case Dim 9's triage rule exists to catch ("referenced issue still open? if
  it guards a closed CRITICAL/HIGH fix → MEDIUM"). The recipe is blind to its
  own highest-value input class. It also makes every published Dim-9 baseline
  systematically low, compounding TD4-2026-08-27-03's separate problem in the
  same figure.
- **Related**: #2262 (CLOSED — same recipe, the whole-repo-textual-scan false
  regression); TD4-2026-08-27-03 (same recipe's published figure being
  unreproducible). Three independent defects in one four-token grep argues for
  fixing it once, properly.
- **Suggested Fix**: Change both occurrences to
  `grep -RIn -E '^[[:space:]]*#\[ignore' --include='*.rs' crates byroredux`
  and record in the SKILL that the baseline steps from 126 to 155 at that
  change, so the next sweep does not read the correction as a regression.

---

### TD4-2026-08-27-05: `_audit-common.md`'s Project Layout omits `byroredux/src/studio_host.rs` and gives `crates/sdk` no layout row

- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `.claude/commands/_audit-common.md:73-81` (the "Binary modules"
  row) and the Project Layout block generally; the unlisted code is
  `byroredux/src/studio_host.rs` (252 LOC) and `crates/sdk/src/` (282 LOC)
- **Status**: NEW — but see the systemic note; a concurrent audit in this run
  filed the sibling instance (`byroredux/src/commands/physics.rs` missing from
  the Commands row), so this is filed for the **pattern plus its two remaining
  instances**, not as a third point fix
- **Age**: `21a840d5` (2026-08-25, "feat: introduce byroredux-sdk for
  renderer-independent tools")
- **Effort**: trivial
- **Description**: `crates/sdk` is correctly present in the crate roster
  (`:142`), the owner map (`:164`) and the un-owned-subsystems table (`:178`) —
  all three added when it landed — but it has no entry in the Project Layout
  block, and its engine-side consumer `byroredux/src/studio_host.rs` appears
  nowhere in the file. The "Binary modules" row enumerates the binary's
  top-level files by name and is the layout's authority for "which file owns
  what"; a 252-LOC new module missing from it is invisible to any audit that
  scopes itself from that list. Crate count (`25`) is correct and gate-checked.
- **Evidence**:
  ```
  $ grep -n "sdk\|studio" .claude/commands/_audit-common.md
  142:nif, papyrus, pex, physics, platform, plugin, renderer, save, scripting, sdk,
  164:| `crates/sdk` | no dedicated owner; ... |
  178:| ByroRedux SDK | `crates/sdk/src/` | Per-domain owner + `/audit-ecs` ... |
  # — three roster/ownership mentions, zero layout rows, and no `studio_host` anywhere.

  $ wc -l crates/sdk/src/*.rs byroredux/src/studio_host.rs
      8 crates/sdk/src/lib.rs
    274 crates/sdk/src/studio.rs
    252 byroredux/src/studio_host.rs
  ```
- **Impact**: Small individually. The pattern is what matters: the layout block
  is hand-maintained, is the first thing every audit reads to scope itself, and
  now has three known gaps from a single fortnight of new modules
  (`commands/physics.rs`, `studio_host.rs`, `crates/sdk`'s layout row). Both
  un-listed modules here belong to `crates/sdk`, the subsystem the same file
  already flags as having no owner audit — so the gap compounds an
  acknowledged coverage hole rather than sitting beside it.
- **Related**: the concurrently-filed `commands/physics.rs` layout gap (same
  mechanism, different row).
- **Suggested Fix**: Add a `Studio/SDK:` layout row naming
  `crates/sdk/src/{lib,studio}.rs` and `byroredux/src/studio_host.rs`, and add
  `studio_host.rs` to the Binary-modules enumeration. Systemically, the crate
  roster is already gate-checked for count (`_audit-validate.sh:205`); the
  cheap generalisation is to extend that check to assert every top-level
  `byroredux/src/*.rs` appears somewhere in the layout block — it is the same
  shape of check, over a list that drifts at the same rate.

---

## Verified Clean

Recorded so the next sweep does not re-derive them.

- **Dim 2 — no duplication found.** Read in full: `crates/sdk/src/studio.rs`
  (new crate, no overlap with `FogBounds`/`PlumeBounds` or any core AABB type),
  the `mesh.rs` chunked-rebuild block (the atomic path is a documented named
  fallback, not a copy), and `pack.rs`'s new `active_package_is!` macro (which
  *removes* fifteen duplicated wrappers). The coordinate-flip consolidation
  verified clean on 08-24 was not re-checked and is assumed to hold.
- **Dim 5 — 0 findings, composition unchanged.** All 20 marker hits are the
  documented exclusion classes: the ESM `XXXX` extended-size protocol tag
  (`esm/reader.rs`, `cell/wrld.rs`, `records/misc/magic.rs`'s sentinel),
  upstream-`FIXME` references (`bgsm/src/bgem.rs:137`,
  `nif/blocks/bs_geometry.rs:596`, `records/misc/world.rs:275`), and two prose
  mentions of closed TODOs (`byroredux/src/scene.rs:1476`,
  `groundcover_translate.rs:252`). Identical to every sweep since 2026-08-16.
- **Dim 6 — 0 findings.** `unimplemented!` / `todo!()` / `panic!("not ` remain
  at 0 workspace-wide. All 48 `stub`/`placeholder`/`not yet` comment hits
  describe intentional design (best-effort parser capture, SpeedTree billboard
  fallback, Vulkan lifecycle notes, test fixtures).
- **Dim 7 — shader-constant provenance holds.** Exactly one `#define` exists in
  any entry-point shader outside the generated header —
  `crates/renderer/shaders/water.frag:144`,
  `#define push waterParams.params[drawPush.waterIndex]`, an accessor alias, not
  a constant. The other 188 all come from
  `shaders/include/shader_constants.glsl`, generated from
  `shader_constants_data.rs` by `build.rs`. No literal bypasses the generator.
- **Dim 3 — the GLSL/Rust GPU-struct pair is in lockstep.** `GpuInstance`'s
  mirror ends `uint _reserved2c; // offset 156, 4 bytes -> total 160`
  (`bindings.glsl:70`), matching `gpu_instance_is_160_bytes_std430_compatible`.
  `exteriorSkyTint` is present in `bindings.glsl:306` and in all four
  re-declaring shaders (`triangle.vert`, `water.vert`, `cluster_cull.comp`,
  `caustic_splat.comp`), matching `gpu_camera_is_368_bytes`. Only *reference
  prose* is stale (TD4-2026-08-27-04 and the concurrently-filed docs sites) —
  the shipped contract is correct.
- **Dim 4 — the path half of the validate gate passes.** 2241 refs across 99
  files (up from 1466/30 — #3202's `docs/engine/*.md` extension landed and is
  working), **0 stale paths**. 8 advisory symbols in audit skills (one real:
  `gpu_camera_is_352_bytes`, filed above; `gl_InstanceID`, `WhiterunDragonsreach`,
  and the four label-name hits are the known benign classes) and 240 in
  `docs/engine` (dominated by CHARAL ruleset game-data identifiers, which are
  authored names not code symbols — a tuning gap in the corpus, not drift).
- **Dim 9 — substance clean.** Every `#[ignore]` sampled, in both the recipe's
  set and the 29 it misses, is data- or GPU-gated. Distribution is unchanged in
  shape (top file: `crates/plugin/tests/parse_real_esm.rs`, 21).
  `byroredux/tests/golden_frames.rs:66` still opts in via `--ignored` with an
  explicit reason. No `#[ignore]` found guarding a closed CRITICAL/HIGH fix.

---

## Deferred

| Finding | Gating reason |
|---|---|
| Full sweep of `crates/facegen`, `crates/mod-runtime`, `crates/hkx`, `crates/fsr3-sys` | Budget — single-agent run over a 9-dimension comprehensive scope with a 152-commit delta. Three of the four saw no `.rs` churn in this window (`git diff --stat 07a029ea..HEAD` over each); `crates/facegen` did change (2 files, +214/-81, via `589d9c02` "actually exercise FO3 in the real-FaceGen corpus test"), so that one crate carries a genuine unmeasured risk of *new* debt, not merely unmeasured standing debt. Worth a `--focus 1,2,7` pass. |
| `context/mod.rs`'s +95 prod-LOC regrowth after #1749 | Recorded in the Dim-1 table rather than filed. It is over threshold and untracked, but the specific constructor #1749 tracked is genuinely gone and 2401 LOC is not yet a materially different situation from the 2306 the 08-24 report accepted. File it if the next sweep shows continued growth. |
| Whether `#3237`'s GRUP-recursion bug is genuinely live | Out of dimension (`/audit-safety` owns it) and this audit's own reading of every recursive walker found them all depth-bounded. Recorded under "Premises investigated" so the two reports are not misread as agreeing. |
| The 22 (of 551) issue numbers inside space-separated `.claude/issues/<A B C>/` directories that lack their own `<N>/` directory | Investigated and judged below the filing bar: 529 of 551 do have their own directory, and `_audit-common.md` explicitly directs auditors to GitHub rather than these local snapshots for issue state (TD10-001 / #1156). |

---

## Deduplication Record

Baseline: `gh issue list --repo matiaszanolli/ByroRedux --limit 500 --state all
--label tech-debt` plus a 300-issue all-label window (#3086–#3398), the two
prior `AUDIT_TECH_DEBT_*.md` reports (08-24 read in full), and eight targeted
`gh issue list --search` queries — one per candidate finding.

**Checked and confirmed still OPEN, not re-filed:**

| Subject | Issue |
|---|---|
| `draw_frame` regrowth (now 3607 prod LOC in a 4947-line file) | #3282 |
| `volumetrics.rs` >2000 production LOC | #2256 |
| `material.rs` crossed 2000 LOC — mostly test growth | #2257 |
| AI-package doc asserts spawn-time-only selection + no pathing | #3351 — **different lines** of the same file as TD3-2026-08-27-01; noted as Related, fix together |
| `ALIAS_FLAG_*` 20/25 unreachable (`quest.rs`, 26 sites) | #2982 |
| EX-16 remaining work (the live gate on `PersistentRefIndex`) | #2372, #3299 |

**Checked and confirmed CLOSED / fixed, verified against live code rather than
trusted from the title:**

| Subject | Issue | Verification |
|---|---|---|
| Validate gate blind to `docs/engine/*.md` | #3202 | gate now globs `docs/engine/*.md`; 2241 refs / 99 files, up from 1466 / 30 |
| Whole-repo `#[ignore]` textual-scan false regression | #2262 | recipe is `--include='*.rs'`-scoped in the SKILL; a *different* defect remains (TD9-2026-08-27-01) |
| 14 `active_package_is_*` / `active_*_location` PACK selectors dead | #3042 | deleted by `3aebf414`; seven survive as `#[cfg(test)]` macro shims. The *doc* was not updated — TD3-2026-08-27-01 |
| EX-14/15 persistent refs | #2369 | closed 2026-08-26 **without** wiring `PersistentRefIndex` — TD8-2026-08-27-01 |
| `VulkanContext::new()` 1025-LOC constructor | #1749 | `init.rs`/`teardown.rs` split holds; `mod.rs` regrowing (+95 prod LOC) |
| 32% of closed issues had no citing commit | #3218 | `--window` mode present and correct — but covers only the forward direction; TD4-2026-08-27-01 is the inverse |
| `#3298` chunked rebuild publishes stale compacted offsets | #3372 | fixed in `cd1aa9e9`; the code that grew `mesh.rs` past threshold — TD1-2026-08-27-01 |

**Filed by a concurrent audit in this same run — deliberately not re-filed:**
GPU-struct doc drift in `docs/engine/shader-pipeline.md:248,193` and
`memory-budget.md:31`; `memory-budget.md`'s missing #3298 two-generation peak;
`docs/feature-matrix.md:251,266-268`; `audit-character/SKILL.md:188`
(`DerivedStatFormula` at 32 B vs 36 B); `nifal.md` + SKILL 18-vs-22 texture
roles; `ui.md:297-301` 6-vs-8 attachments; `commands/physics.rs` absent from
`_audit-common.md`'s Commands row.

---

## Next Step

```
/audit-publish docs/audits/AUDIT_TECH_DEBT_2026-08-27.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=2 LOW=7
