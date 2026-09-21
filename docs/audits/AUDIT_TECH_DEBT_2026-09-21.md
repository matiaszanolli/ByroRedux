# Tech-Debt Audit — 2026-09-21

**HEAD**: `29a130e19` · **Baseline**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`, 7 days / 229 crate-commits earlier) · **Audited**: all 9 dimensions, delta-first (deep) · **Unchanged since baseline (skimmed)**: Dim 5 markers (22, unchanged; 5 of 6 non-`XXXX` are documented false positives), Dim 6 (`unimplemented!`/`todo!()` still **0**), Dim 7 (one bare `data.len() == 72` in the delta — the documented FO3/FNV MGEF 72-byte DATA arm, pre-existing), Dim 9 (feature gates all carry CI lanes).

Orchestrator-run (sequential dimensions, no sub-agents). One trivial gate fix and one
session-regression fix were applied inline during the audit and are committed separately;
everything else is reported, not fixed.

## Executive Summary

| Severity | Count | Delta vs 09-14 |
|---|---|---|
| HIGH | 0 | = |
| MEDIUM | 1 | new (clippy gate red) |
| LOW | 4 | net new (file membership rotated) |

- The `cargo clippy --workspace -- -D warnings` CI gate is **red on rustc/clippy 1.96** —
  the exact new-toolchain-lint class the 09-14 audit predicted (commit `800802516`). One
  hard error + 10 warnings across three crates. Two trivial pieces fixed inline this audit;
  the menuxml bucket is the remainder.
- The oversized-file roster rotated: `groundcover.rs` **newly crossed** the 2000
  production-LOC line (1966 → 2067, the #4297 ground-cover week); `storage_util.rs` grew
  further (2160 → 2320, open #4218); `scene.rs` sits exactly ON the line (2001).
- Young-crate sweep (sdk / mod-runtime / menuxml / scripting / save / hkx / spt): every
  crate carries real unit tests (22–104 per crate); the debt signal is menuxml's missing
  clippy pass, not absence of coverage.

## Baseline Snapshot (2026-09-21)

```text
markers: 22                    (09-19: 22 — flat; 16 XXXX false positives, 5 documented FPs, 1 fresh)
allow(dead_code): 28           (09-19: 28 — flat)
unimplemented!/todo!(): 0      (09-19: 0 — still zero; a fresh hit would be notable)
#[ignore] tests: 221           (09-19: 217 — +4, all data-gated: sound archives / Oblivion data / FO4 data / FNV+Oblivion masters)
files >2000 production LOC: 3  (09-19: 3 — membership rotated, see TD1)
test files >2000 total LOC: 54 (09-19: 52)
```

`_audit-validate.sh`: **OK** — all path references valid (266 advisory symbols, the
long-standing advisory backlog; count unchanged in character). `gpu_material_size_claims`
green. `GpuInstance`/`GpuCamera` prose figures clean against the layout pins. The four
2026-09-11 doc-rot issues (#4145/#4146/#4200/#4202) were verified fixed-at-HEAD and closed
during this audit's `/fix-issue` pre-run — Dim 3's prior-year backlog is clear.

## Top Quick Wins (trivial/small)

1. **menuxml clippy pass** (TD8-01, trivial ×7): two `too_many_arguments` raster fns get
   the house `#[allow(clippy::too_many_arguments)]`-with-reason, two
   `only_used_in_recursion` params get underscored or dropped, `needless_range_loop`,
   `match_like_matches_macro`, `then`-in-`filter_map` are mechanical.
2. **plugin clippy pair** (TD8-01, trivial): `items.rs:1006` match-equality → `if`;
   `cell/support.rs:669` collapsible match arm.
3. **groundcover.rs split** (TD1-01, small): see below — 67 lines over, one evening.
4. **scene.rs decision** (TD1-03, small): 2001 prod LOC — split or formally watch.

## Medium Investments

1. **storage_util.rs (2320, Existing: #4218)** — grew +160 in one week (new list
   declaration builders). The #4218 split axis stands: declaration-builder stack
   (`papyrus_storage_util_*_declarations`, ~570 lines) vs adaptation structs vs
   per-type command builders. The file is the SDK's hottest surface; every week of
   growth adds to the merge cost.
2. **groundcover.rs (2067)** — split axis chosen from the file's structure, not a tag:
   `impl GroundCoverPipeline` has two blocks (construct + descriptors at 493, record at
   900 → `record_scatter`/`record_interaction`/`record_draw` to 1907) plus the
   `GroundCoverStats`/counter-offset telemetry cluster (289–414). The volumetrics
   construct-vs-record-vs-teardown precedent applies directly; stats is a clean third
   file. Effort: small.

## Findings

### TD8-2026-09-21-01: clippy -D-warnings gate red on rustc 1.96 (1 error + 10 warnings)
- **Severity**: MEDIUM (CI gate class; the skill's own 09-14 prediction landed)
- **Dimension**: 8 — Dead Code & Backwards-Compat Cruft
- **Location**: `crates/renderer/src/vulkan/texture.rs:191` (error, `unsafe` block
  missing a SAFETY comment); `crates/menuxml/src/{raster.rs:85,198, tex.rs:277,
  eval.rs:305,354, parse.rs:475,476}` (7 warnings);
  `crates/plugin/src/esm/records/items.rs:1006`,
  `crates/plugin/src/esm/cell/support.rs:669` (2 warnings)
- **Status**: NEW (the error; the menuxml bucket is the young-crate's first clippy pass)
- **Description**: rustc/clippy 1.96 raises `unsafe_block_missing_safety_comment` on
  `texture.rs:191` (the one-shot upload-command barrier block — the house SAFETY
  convention requires the comment regardless) and a batch of on-by-one lints on
  `menuxml` (never clippy-passed since its M48.4 landing) plus two plugin lints.
- **Evidence**: `cargo clippy --workspace` on the 1.96.0 toolchain: 1 error
  (byroredux-renderer fails to compile under `-D warnings`), 10 warnings.
- **Impact**: any contributor on a current toolchain gets a red gate on untouched code;
  the menuxml warnings will multiply.
- **Fixed inline during this audit**: the `texture.rs:191` SAFETY comment, and the
  `emitter.rs` collapsible-`?` warning (`crates/nif/src/import/walk/emitter.rs:437`) —
  that one was a regression introduced by this week's own #4401 fix, caught by the same
  sweep.
- **Suggested Fix**: menuxml allow-with-reason pass on the two raster fns + mechanical
  fixes for the five one-liners; plugin pair likewise. Effort: trivial each.
- **Related**: #800802516 (toolchain precedent)

### TD1-2026-09-21-01: groundcover.rs crossed the 2000 production-LOC line (2067)
- **Severity**: LOW · **Dimension**: 1 — File/Function/Module Complexity
- **Location**: `crates/renderer/src/vulkan/groundcover.rs` (tests at :2069; prod 2067,
  was 1966 at the last audit) · **Status**: NEW (newly crossed) · **Effort**: small
- **Description**: the #4297 ground-cover week added ~1200 changed lines here. The two
  `impl GroundCoverPipeline` blocks (construct/descriptors vs record) plus the
  `GroundCoverStats` counter-offset telemetry cluster are three responsibilities in one
  file.
- **Suggested Fix**: `groundcover/` directory split on the volumetrics precedent —
  `pipeline.rs` (types + construct, 63–~900), `record.rs` (`record_scatter` /
  `record_interaction` / `record_draw`, 1562–1907), `stats.rs` (289–414). No barrier or
  ordering change involved (feedback-speculative-vulkan-fixes not triggered).
- **Related**: AUDIT_TECH_DEBT_2026-09-14 TD8 (blade-stride mirror, same file)

### TD1-2026-09-21-02: storage_util.rs grew again (2320 prod LOC, +160 in a week)
- **Severity**: LOW · **Dimension**: 1 · **Location**:
  `crates/sdk/src/compatibility/storage_util.rs` · **Status**: **Existing: #4218 (OPEN)**
  · **Effort**: medium
- **Description/axis**: unchanged from #4218 — declaration-builder stack (~570 lines) vs
  adaptation structs vs per-type command builders. Recorded: the file grew +160 lines
  this week alone (new list-adaptation builders), so the split cost rises weekly.

### TD1-2026-09-21-03 (watch): scene.rs sits exactly on the line (2001)
- **Severity**: LOW · **Dimension**: 1 · **Location**: `byroredux/src/scene.rs`
  · **Status**: NEW (watch-list row) · **Effort**: small
- **Description**: 2001 prod LOC — one line over. The last audit tracked this file's
  sub-section growth (996 → 1056 inside :720-1775). Watch-list per the skill; the next
  feature landing here forces the split decision. Watch-list companions within 5%:
  `crates/plugin/src/esm/records/actor/mod.rs` (1995),
  `crates/renderer/src/vulkan/context/draw.rs` (1985),
  `crates/nif/src/import/types.rs` (1929).

### TD2-2026-09-21-01: menuxml raster has no shared blit-texture preflight
- **Severity**: LOW · **Dimension**: 2 — Logic Duplication
- **Location**: `crates/menuxml/src/raster.rs:85` (`blit`), `:198` (`text_line`)
- **Status**: NEW · **Effort**: small
- **Description**: both 9-arg entry points repeat the same preflight shape
  (clip-window intersect → tiled-vs-natural extent resolution → per-tile source
  rect → `crop` wrap) before diverging. Consolidation site: one
  `raster_preflight(tex, dst, crop, clip) -> Option<TilePlan>` helper both consume.
- **Related**: TD8-01 (the `too_many_arguments` warnings are the same two fns — a
  shared params struct fixes both the lint and the duplication)

## Deferred

- **FO76/Starfield exterior harness debt** — waits on exterior support landing
  (#4507's policy-skip banners are the current state; the fixtures land with support).
- **266 advisory symbols** in `docs/engine/*.md` — long-standing italicisation backlog;
  the gate passes on paths, and the count did not grow this cycle.
- **Open tech-debt issues** #4563 / #4552 (NIFAL 2026-09-21) and #4457 (TPLT hoist) —
  owned by the NIFAL/character tracks, deduped out of this report.

## Verification notes

- `prod_loc` self-test: ok. Baseline counts re-measured, not quoted.
- Clippy inventory taken on the 1.96.0 toolchain (`rustup which --toolchain 1.96.0`).
- The three 2026-09-11-era doc-rot issues and both ESM findings were resolved during
  this audit's pre-run `/fix-issue #4145-#4207`; their closures (fixed-elsewhere in
  `d574d9bd1`, plus the new pins) are deduped OUT of this report's findings.
