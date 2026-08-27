=== 2367 ===
## #2367 — PERF-REGRESSION-3a02b02d..28155b79: FO4 scenes (MedTek/Dugout) ~33-34% slower at flat entity count; Prospector (FNV) ~2x faster — needs bisection
State: OPEN
Labels: bug medium performance game:fnv game:fo4 

**Severity**: MEDIUM
**Dimension**: Performance / Bench-of-record (ROADMAP #2279 refresh)
**Location**: unknown — needs bisection across `3a02b02d..28155b79` (119 commits, spans Session 60-62 including procedural volumetric fog, clustered local fog volumes, material-aware path-traced GI extensions, materials-pipeline refactor `ImportedMaterial`/`MaterialTextureSet<T>`, streaming-resumability mitigations)

## Description

Refreshing the ROADMAP bench-of-record (#2279) surfaced large swings against the prior record (`3a02b02d`, 2026-07-26). Per this project's own standing methodology (documented in ROADMAP's Bench-of-record section), a same-session same-machine worktree rebuild of the prior commit was run as a control before drawing any conclusion — this is what separated PERF-REGRESSION-6c56e311 (#2161-adjacent) from machine noise previously, and does the same here.

Control (`3a02b02d` rebuilt in a worktree) vs HEAD (`28155b79`), same session/machine, TAA config, median of 3 runs x 300 frames:

| Scene | Entities (ctrl→HEAD) | TAA frame ms (ctrl→HEAD) | Verdict |
|---|---|---|---|
| Prospector (FNV) | 3626→3626 (flat) | 14.69→7.33 | **Real ~2x improvement** |
| Cornell (synthetic control) | 25→27 (flat) | 2.76→3.32 | Real but mild slowdown (~20%) |
| Whiterun (Skyrim SE) | 3406→5150 (+51%) | 9.99→15.37 | Confounded by entity growth — not conclusive |
| MedTek Research 01 (FO4) | 31495→31400 (flat) | 40.17→53.58 | **Real ~33% regression, flat content** |
| Dugout Inn (FO4) | 6978→6978 (flat) | 30.44→40.79 | **Real ~34% regression, flat content** |

The control run reproduces the original `3a02b02d` ROADMAP figures closely (e.g. Prospector 65.3→68.1 FPS, Dugout 31.9→32.9 FPS — within normal same-machine noise), which is what makes the HEAD deltas trustworthy rather than contention artifacts.

## Evidence

Full control-run report and HEAD report available in this session's bench output (`target/fsr-bench/raw.tsv` at both commits). Both regressed scenes are Fallout 4 content; the dramatically improved scene is FNV; the synthetic control is only mildly affected — this points at something FO4-specific rather than a universal engine regression, but that is a pattern observation, not a root cause.

## Impact

Two real Fallout 4 interior scenes are ~33-34% slower in frame time at byte-identical entity counts. Whiterun's entity count grew +51% over the same range (3406→5150) for reasons not yet understood — worth separately investigating since it could itself be either a content-loading behavior change or a symptom of the same underlying cause.

## Suggested Fix

Bisect `3a02b02d..28155b79` using `scripts/fsr-bench-matrix.sh` restricted to Dugout (smaller/faster-loading of the two regressed FO4 scenes, better bisection candidate than MedTek) at TAA-only to narrow the commit range efficiently. Prime suspects given the commit range's content: the procedural volumetric fog / clustered local fog volumes work and the material-aware path-traced GI extensions (both Session 62), since those are the kind of per-fragment cost additions that would hit FO4's higher-poly interiors harder — but this is a hypothesis, not yet verified.

## Completeness Checks
- [ ] **BISECT**: Narrow `3a02b02d..28155b79` to the actual introducing commit(s)
- [ ] **WHITERUN-ENTITY-COUNT**: Understand why Whiterun's loaded entity count grew 3406→5150 (+51%) over the same range
- [ ] **PROSPECTOR-IMPROVEMENT**: The ~2x Prospector improvement is also unexplained — confirm it's real engine work and not a measurement artifact before citing it as a win
- [ ] **TESTS**: N/A until root-caused — this is a measurement/bisection issue, not a code-fix issue yet

=== 3005 ===
## #3005 — RT-2026-08-16-06: draw-batch merge regression on fnv and fo3 — batches and GPU calls past the x1.1 contract
State: OPEN
Labels: bug renderer medium performance game:fnv game:fo3 

Filed from `docs/audits/AUDIT_RUNTIME_2026-08-16.md` (runtime telemetry baseline diff).

**Location**: `.claude/audit-baselines/runtime/fnv-FreesideAtomicWrangler.tsv` · `.claude/audit-baselines/runtime/fo3-MegatonPlayerHouse.tsv` · `byroredux/src/render/mod.rs`

## Description

The runtime telemetry sweep measured `fnv` and `fo3` past the `≤ baseline ×1.1` contract on the **draw-batch merge and GPU-call halves**, while the `DrawCommand` count itself stayed within tolerance. That combination points at the merge step rather than at more geometry being submitted.

## Evidence

Measured by `/audit-runtime` against the checked-in baselines on 2026-08-16; both scenes exceed the ×1.1 gate on `batches` and `gpu_calls` while `cmds` remains inside it. Full per-metric table in the source report (§ RT-2026-08-16-06).

Baselines re-confirmed present 2026-08-17: `fnv-FreesideAtomicWrangler.tsv`, `fo3-MegatonPlayerHouse.tsv`.

## Impact

More batches and more GPU calls for the same draw-command count means the batching pass is merging less effectively than the baseline recorded — CPU-side submission cost rises with no rendering benefit. On the user's hardware (Ryzen 7950X), a CPU-side regression is the kind that shows up as a frame-time floor rather than a GPU stall.

## Suggested Fix

Bisect `byroredux/src/render/mod.rs`'s batch-merge path against the baseline commit to find what changed the merge key or its ordering. Then either fix the regression or, if the new behaviour is correct, re-baseline **with the justification recorded** — a silent re-baseline would erase the signal.

## Related

- #3006 (RT-2026-08-16-07 — the FO4 scene's different regression shape)

## Completeness Checks
- [ ] **CAUSE-NOT-BASELINE**: The regression is explained before any re-baseline
- [ ] **SIBLING**: The other three baseline scenes checked for the same merge-side drift
- [ ] **RE-BASELINE-JUSTIFIED**: If re-baselined, the reason is recorded in the TSV's companion notes
- [ ] **TESTS**: The telemetry gate fails on this metric until resolved


=== 3039 ===
## #3039 — FNV-2026-08-16-D8-01: the playable-slice smoke gates are Skyrim-only; the reference title has none
State: OPEN
Labels: bug medium legacy-compat tech-debt game:fnv game:skyrim 

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 8 — Runtime gates).

**Location**: `docs/smoke-tests/p0-door-interaction.sh`:19 and the P1/P2 siblings

## Description

All three P0/P1/P2 gates for the project's active execution focus (the playable vertical slice) hard-code `SKYRIM_DATA` and skip when `Skyrim.esm` is absent.

**FNV — the project's declared reference title — has no playable-slice gate at all.** It has only a ragdoll gate (`docs/smoke-tests/m41-ragdoll.sh`:33, `FNV_DATA`).

## Evidence

Live probe on the FNV bench-of-record cell shows the slice is not merely ungated but **non-functional**:

```
byro> combat.approach 7
"combat.approach: entity 7 is not a damageable actor"
byro> input.press attack
"input.press: queued Attack through the R binding"
```

Re-verified 2026-08-17.

## Impact

Nothing exercises door interaction, character traversal or melee combat against FNV content — on the game the engine is calibrated against.

This is why #2986 (FO3/FNV actors get no `ActorValues`/`ActorVitals`) and #3004 (no Health term in the auto-calc) could both be true without any gate turning red. The probe output above is those two findings observed from the outside.

## Suggested Fix

Add FNV variants of the three playable-slice gates, parameterised on `FNV_DATA` — or better, parameterise the existing scripts by game so a new title costs a fixture rather than a script copy.

Note the gates will be RED for FNV until #2986 and #3004 land; that is the correct state, and worth landing the gate to make it visible.

## Related

- #2986 (ESM-D7-01), #3004 (RT-05) — the two findings this missing gate concealed
- #3003 (RT-04 — no CI runs any gate), #3001 (RT-02 — two gates already RED)

## Completeness Checks
- [ ] **PARAMETERISED**: The gate is game-parameterised rather than copy-pasted per title
- [ ] **HONEST-RED**: The FNV gate is allowed to fail until #2986/#3004 land, not weakened to pass
- [ ] **SKIP≠PASS**: Paired with #3003 so a data-less run is distinguishable
- [ ] **TESTS**: The FNV gate exercises door, traversal and melee as the Skyrim ones do


=== 3042 ===
## #3042 — FNV-2026-08-16-D9-01: the 14 active_package_is_* / active_*_location PACK selectors are dead
State: OPEN
Labels: bug import-pipeline low legacy-compat tech-debt game:fnv esm-plugin 

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 9 — AI Packages & Procedure Runtimes).

**Location**: `crates/plugin/src/esm/records/misc/pack.rs`:396-… (14 `pub fn`s), re-exported at `crates/plugin/src/esm/records/misc.rs`:67-68 and `crates/plugin/src/esm/records/mod.rs`:60-61

## Description

#2031 collapsed the spawn tail into a single `active_package` resolve plus the `is_sandbox()/is_wander()/…` else-if chain in `byroredux/src/npc_spawn/ai_package.rs`:106-146.

The seven `active_package_is_*` predicates and seven `active_*_location`/`active_*_target` accessors they replaced were **left in place**. A workspace-wide search finds **no call expression** for any of the 14.

`pub` visibility suppresses the dead-code lint, so nothing surfaces it.

## Evidence

Re-verified 2026-08-17 — every surviving reference is a `use` statement or a comment, never a call:

```
$ grep -rn "active_package_is_sandbox\|active_sandbox_location" crates/ byroredux/ --include="*.rs" | grep -v pack.rs
crates/plugin/src/esm/records/mod.rs:60:    active_package_is_patrol, active_package_is_sandbox, …   <- use
crates/plugin/src/esm/records/misc.rs:67:   active_package_is_patrol, active_package_is_sandbox, …   <- use
crates/plugin/src/esm/records/actor/tests.rs:696:  /// … `active_package_is_sandbox` always            <- comment
crates/plugin/src/esm/records/actor/tests.rs:730:  /// … `active_package_is_sandbox` looks up           <- comment
```

Same shape for all seven pairs.

## Impact

~150 lines of unreachable public API that still reads as the live selection mechanism. `/audit-fnv`'s own Dimension 9 entry-point list names these selectors, and a future contributor extending package selection will reasonably edit the dead copy.

**Note a doc consequence**: `.claude/commands/_audit-common.md`'s Sandbox AI row states *"the spawn-tail reads all seven `active_package_is_*`/`active_*_location`/`active_*_target` selector pairs"* — that is now stale, and is the kind of claim that would send an auditor to the dead code.

## Suggested Fix

Delete all 14 plus their two re-export lines, keeping `active_package` and `PackRecord::is_*`. Or, if they are wanted as a public plugin-crate API, add a test exercising each so the intent is recorded.

Update `_audit-common.md`'s Sandbox AI row either way.

## Related

- #2031 (the collapse that orphaned them)
- Overlaps `/audit-tech-debt`'s dead-code dimension (#2982 is the same shape in `quest.rs`)

## Completeness Checks
- [ ] **ALL-14**: Every selector removed (or tested), not a subset
- [ ] **RE-EXPORTS**: The two `use` lines in `misc.rs` and `mod.rs` removed with them
- [ ] **SKILL-DOC**: `_audit-common.md`'s Sandbox AI row corrected — it currently points at the dead path
- [ ] **PATH-GATE**: `.claude/commands/_audit-validate.sh` still passes after the skill edit
- [ ] **TESTS**: `cargo test -p byroredux-plugin` green after removal


