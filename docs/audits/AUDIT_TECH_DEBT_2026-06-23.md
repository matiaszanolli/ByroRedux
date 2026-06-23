# Tech-Debt Audit — 2026-06-23

9-dimension sweep (run inline, single-agent — no nested Task agents). Prior
report: [2026-06-14](AUDIT_TECH_DEBT_2026-06-14.md). This cycle's mandate
explicitly reaches the three young crates that had not yet seen a debt sweep:
`crates/pex/` (M47.2 `.pex` decompiler), `crates/save/` (M45 save/load), and the
expanded `crates/scripting/` (M47.1/M47.2 recognizer chain).

---

## 1. Executive Summary

**9 findings** — **0 CRITICAL, 0 HIGH, 1 MEDIUM, 8 LOW**.

The single MEDIUM (TD3-001) is the **known `docs/feature-matrix.md` doc-rot**: two
status rows still read "unstarted"/"transpiler unstarted" for milestones that have
**shipped** (M45/M45.1 save/load → `crates/save/`; M47.2 compiled-`.pex` slice →
`crates/pex/` + `crates/scripting/`). It is promoted to MEDIUM under the severity
table ("stale doc baseline that misled an audit in the last 90 days") because the
matrix is the canonical at-a-glance status surface and would mislead the *next*
per-game/scripting audit into re-scoping finished work.

**The new crates are clean.** `crates/pex/`, `crates/save/`, and the expanded
`crates/scripting/` carry **zero** TODO/FIXME/HACK markers, **zero**
`#[allow(dead_code)]`, **zero** `unimplemented!()/todo!()`, and **no file over
2000 LOC** (largest is `quest_stage_gate.rs` at 763, of which only 311 is
production — the rest is test). Young code, but written to standard.

The debt profile is otherwise **unchanged in character from 06-14 and 05-28**:
file/function growth in the two BLOCKED Vulkan-recording monoliths and the three
binary-crate files, all already tracked by **OPEN umbrella issues**. This cycle
they grew *further*: `draw.rs` **3831 → 4176** (+345), `asset_provider.rs`
**3014 → 3405** (+391, was 2561 two cycles ago), `context/mod.rs` **3142 →
3275** (+133), `main.rs` **2720 → 2789** (+69). `draw_frame()` is now a single
**~3360-LOC function**.

Recurring traps **all hold**: the GpuCamera/GpuInstance/GpuMaterial doc-size
claims (336/112/300 B) verified correct against their pinned tests; the
`Material::classify_pbr` doc comments all correctly frame the symbol as
deleted/historical (the recurring rot is *not* present); the 05-28→06-14
`context/mod.rs` 304-B GpuCamera regression (last cycle's TD3-001) is **fixed**;
shader `#define`s are all sourced from the generated
`shaders/include/shader_constants.glsl`; the path-validation gate passed clean
(979 refs, 0 stale).

One genuinely new finding worth acting on: `mswp::peek_path_filter` is dead
(zero callers) and its reservation comment cites **#584 / FO4-DIM6-02, which is
CLOSED** — the breadcrumb has outlived its driver (TD8-001).

| Severity | NEW | Existing/Regression | Total | Dimensions |
|----------|-----|---------------------|-------|------------|
| CRITICAL | 0   | 0                   | 0     | — |
| HIGH     | 0   | 0                   | 0     | — |
| MEDIUM   | 1   | 0                   | 1     | D3 |
| LOW      | 2   | 6                   | 8     | D1, D3, D5, D8 |

Delta vs 2026-06-14: the file-growth findings continue to restate OPEN umbrella
issues (#1323 / #1669–#1673) with materially-worse LOC; one new dead-code
breadcrumb (TD8-001); one new dead-fn (TD8-001 covers it). No new correctness
rot. New-crate sweep added zero findings (clean).

---

## 2. Baseline Snapshot

Source: `/tmp/audit/tech-debt/baseline.txt` (captured 2026-06-23 pre-sweep).

| Metric | 2026-06-23 (today) | 2026-06-14 | Δ |
|---|---:|---:|---:|
| `TODO`/`FIXME`/`HACK`/`XXX` (raw grep) | 18 | 20 | −2 |
| ↳ of which *active production markers* | **1** (`material.rs:608`, tracked by OPEN #1627) | 3 | −2 |
| `#[allow(dead_code)]` | 21 | 31 | −10 |
| `unimplemented!()` / `todo!()` | **0** | 0 | 0 |
| `panic!("not …")` | 0 | 0 | 0 |
| `#[ignore]` tests (raw, excl. target/) | 261 | 247 | +14 |
| ↳ genuine debt (not Vulkan/data gate) | **0** | 1 | −1 |
| Files > 2000 LOC | **6** | 6 | 0 |

`#[allow(dead_code)]` dropped 31 → 21 (genuine cleanup since last cycle —
`components.rs` removed its per-field guards per its own comment). The marker
count fell to a single active production marker, itself already issue-tracked.

### Files > 2000 LOC (current set)

```
4176  crates/renderer/src/vulkan/context/draw.rs      (+345 vs 06-14)
3405  byroredux/src/asset_provider.rs                 (+391 vs 06-14)
3275  crates/renderer/src/vulkan/context/mod.rs       (+133 vs 06-14)
2789  byroredux/src/main.rs                           (+69  vs 06-14)
2131  crates/nif/src/blocks/particle.rs               (+6   vs 06-14)
2072  crates/nif/src/import/collision.rs              (net-new entrant this set)
```

---

## 3. Top 10 Quick Wins (trivial/small)

1. **TD3-001** — fix the two `docs/feature-matrix.md` rows (M45 "unstarted",
   M47.2 "transpiler unstarted") to reflect shipped state. (trivial)
2. **TD8-001** — delete `mswp::peek_path_filter` (zero callers; reserved for a
   CLOSED issue). (trivial)
3. **TD5-001** — the lone active production marker (`material.rs:608` glass
   transmission TODO) is already tracked by OPEN #1627; no action beyond keeping
   the link accurate. (no-op / dedup note)
4. **TD3-002** — `feature-matrix.md:175` "Papyrus transpiler (M47.2) … M47.2
   (Tier 3)" in the "What Doesn't Work Yet" table should be reframed to "`.pex`
   recognizer slice shipped; full transpiler deferred." (trivial)

(Only four quick wins exist this cycle — the marker/dead-code surface is the
cleanest it has been across the last three audits.)

## 4. Top 5 Medium Investments (file/function splits)

All five are **already OPEN umbrella issues**; this audit refreshes their LOC
and confirms continued growth. No new split axes proposed (the prior cycles'
axes still apply).

1. **#1671 / TD9-NEW-06** — `draw_frame()` ~3360 LOC inside `draw.rs` (4176).
   Per-pass recording group extraction (geometry / RT / denoise / composite /
   overlay). *Render-pass-adjacent — see `feedback_speculative_vulkan_fixes.md`.*
2. **#1669 / TD9-NEW-02** — `asset_provider.rs` 3405 LOC. Split BSA/BA2
   resolution vs TextureProvider vs mesh extraction (+ the new `extract_pex`).
3. **context/mod.rs** 3275 LOC, `new()` 1025 LOC — struct+`new()` vs `Drop` vs
   accessors (covered under the same #1323 umbrella).
4. **#1670 / TD9-NEW-04** — `main.rs` 2789 LOC / `App::new` 626 LOC — boot vs
   event-loop vs system wiring.
5. **particle.rs** 2131 / **collision.rs** 2072 — typed-struct vs opaque-fallback
   split axes (#1323 umbrella).

---

## 5. Findings

### TD3-001: feature-matrix.md M45 + M47.2 rows read "unstarted" for shipped milestones
- **Severity**: MEDIUM (promoted: stale doc baseline that would misdirect the next audit)
- **Dimension**: 3 (Stale Documentation)
- **Location**: `docs/feature-matrix.md:139`, `docs/feature-matrix.md:176`
- **Status**: NEW (known target, called out in the skill + `_audit-common.md`)
- **Description**: Line 139 ("Full Papyrus transpiler (M47.2)") reads
  "✗ Foundation done; transpiler unstarted". Line 176 ("Save / load (M45)")
  reads "M45 (unstarted)". Both milestones have shipped.
- **Evidence**: `crates/save/src/{snapshot,registry,disk,validate,driver,lib}.rs`
  exist; commits `bd2d0de2 feat(save): M45 — full-ECS-snapshot save/load` and
  `48e18c4f feat(save): M45.1 — live load-apply` landed. `crates/pex/src/`
  (opcode/reader/model/decompile) exists; commits `fcd46e90 feat(scripting): wire
  VMAD .pex scripts through the recognizer at cell load (M47.2)`,
  `92560525 test(m47.2)`, `f1a00e89 feat(cell): M47.2 script-attach summary`
  landed. `_audit-common.md` already flags this matrix as lagging.
- **Impact**: The matrix is the canonical per-game/scripting status surface; a
  reader (or the next `/audit-scripting` / `/audit-save` run) would conclude the
  work is unstarted and re-scope finished milestones.
- **Suggested Fix**: M47.2 row → "✓ `.pex` recognizer slice (CFG→lift→
  control-flow→lower→short-circuit); full transpiler deferred". M45 row →
  "✓ full-ECS-snapshot save + M45.1 live load-apply". Update the "What Doesn't
  Work Yet" table (TD3-002) to match.

### TD3-002: feature-matrix "What Doesn't Work Yet" table still lists M45/M47.2 as gaps
- **Severity**: LOW
- **Dimension**: 3 (Stale Documentation)
- **Location**: `docs/feature-matrix.md:175-176`
- **Status**: NEW
- **Description**: The gaps table lists "Papyrus transpiler (M47.2) … Script
  execution on real content" and "Save / load (M45) … Game sessions persist
  … M45 (unstarted)" as live gaps "as of 2026-06-02". The `.pex` recognizer
  slice and M45/M45.1 both shipped after that date.
- **Evidence**: Same commit set as TD3-001.
- **Impact**: Same surface as TD3-001; redundant rot in the same file.
- **Suggested Fix**: Remove the Save/load row from the gaps table; reframe the
  transpiler row as "full Papyrus transpiler deferred — `.pex` recognizer slice
  shipped (M47.2)".

### TD8-001: mswp::peek_path_filter is dead, reserved for a CLOSED issue
- **Severity**: LOW
- **Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
- **Location**: `crates/plugin/src/esm/records/mswp.rs:150-155`
- **Status**: NEW
- **Description**: `peek_path_filter` carries
  `#[allow(dead_code)] // Reserved for the FO4-DIM6-02 stage-2 cell-loader
  integration.` It has **zero callers** anywhere in the workspace, and
  FO4-DIM6-02 (#584, "TXST.MNAM parsed but never resolved at REFR time") is
  **CLOSED**. The reservation breadcrumb has outlived its driver.
- **Evidence**: `grep -rn 'peek_path_filter'` returns only the definition.
  `gh issue` shows #584 CLOSED.
- **Impact**: Pure rot — a `pub(crate) fn` no one consumes, with a stale
  forward-reference. CLAUDE.md: delete, no breadcrumbs.
- **Suggested Fix**: Delete the function and the comment. If MSWP path-filter
  peeking is genuinely needed by a *future* cell-loader path, re-add it at that
  call site under a *live* issue.

### TD1-001: draw.rs / draw_frame continues to grow (4176 file / ~3360 fn)
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `crates/renderer/src/vulkan/context/draw.rs`
- **Status**: Existing: #1671 (TD9-NEW-06 split axis) / #1323 (umbrella)
- **Description**: The single largest file and function in the workspace grew
  +345 LOC since 06-14. `draw_frame()` is ~3360 LOC of per-pass command
  recording in one function.
- **Evidence**: `wc -l` = 4176; the prior report recorded 3831 / ~3211.
- **Impact**: Every renderer edit, review, and merge taxes this file; the
  recording order is hard to reason about as a single block.
- **Suggested Fix**: No new action — already scoped on #1671. Render-pass
  recording splits must be RenderDoc-verified, not speculative
  (`feedback_speculative_vulkan_fixes.md`).

### TD1-002: asset_provider.rs grew to 3405 LOC
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `byroredux/src/asset_provider.rs`
- **Status**: Existing: #1669 (TD9-NEW-02) / #1323
- **Description**: +391 LOC since 06-14 (was 2561 at #1323 filing). The new
  `extract_pex` path (M47.2 compiled-script attach) added to an already-oversized
  file. `fill` (567 LOC) and `resolve` (411 LOC) are the largest functions.
- **Evidence**: `wc -l` = 3405.
- **Impact**: Mixing BSA/BA2 archive resolution, TextureProvider, mesh
  extraction, and now `.pex` extraction in one file.
- **Suggested Fix**: No new action — #1669 split axis (BSA/BA2 vs
  TextureProvider vs mesh-extraction) now also wants a `.pex`/script-extraction
  bucket.

### TD1-003: context/mod.rs grew to 3275 LOC; new() at 1025 LOC
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `crates/renderer/src/vulkan/context/mod.rs`
- **Status**: Existing: #1323 (umbrella)
- **Description**: +133 LOC since 06-14. `VulkanContext::new()` is a single
  1025-LOC init function.
- **Evidence**: `wc -l` = 3275; `new()` spans ~1025 lines.
- **Impact**: Init chain hard to read/audit as one function.
- **Suggested Fix**: No new action — struct+`new()` vs `Drop` vs accessors axis
  under #1323. Vulkan-init ordering is correctness-sensitive — extract
  sub-builders, don't reorder.

### TD1-004: main.rs grew to 2789 LOC
- **Severity**: LOW
- **Dimension**: 1 (Complexity)
- **Location**: `byroredux/src/main.rs`
- **Status**: Existing: #1670 (TD9-NEW-04) / #1323
- **Description**: +69 LOC since 06-14. `App::new` (626 LOC), `about_to_wait`
  (619 LOC), `render_one_frame` (307 LOC) are the large functions.
- **Evidence**: `wc -l` = 2789.
- **Impact**: Boot, event-loop, and system-wiring concerns interleaved.
- **Suggested Fix**: No new action — #1670 axis (boot vs event-loop vs system
  wiring).

### TD5-001: lone active production marker (glass transmission TODO) already issue-tracked
- **Severity**: LOW
- **Dimension**: 5 (Stale Markers)
- **Location**: `crates/renderer/src/vulkan/material.rs:608`
- **Status**: Existing: #1627 (TD5-002, OPEN)
- **Description**: The single active production TODO names the transmission lobe
  not yet on GpuMaterial. The doc comment correctly cites OPEN #1627 and notes
  #1248 closed.
- **Evidence**: `grep` of the marker; #1627 confirmed OPEN in cached issue list.
- **Impact**: None beyond the tracked feature gap; the marker is correctly
  linked to a live issue, so it has *not* outlived its driver.
- **Suggested Fix**: No action — keep the #1627 link accurate. (All other
  TODO/FIXME/XXX hits this cycle are false positives: the `XXXX` ESM
  extended-size protocol tag in `reader.rs`/`magic.rs`, the upstream-reference
  `// FIXME note` in `bgsm/src/bgem.rs` and `bs_geometry.rs`, and the closed-out
  `#1055`/`#242` TODO breadcrumb in `scene.rs:771`.)

### TD8-002: stale-but-harmless `_unused`-bound reads in ESM walkers (informational)
- **Severity**: LOW
- **Dimension**: 8 (Dead Code)
- **Location**: `crates/plugin/src/esm/records/script_instance.rs:256,259`,
  `crates/plugin/src/esm/cell/walkers.rs:1097,1111`,
  `crates/plugin/src/esm/records/items.rs:243`
- **Status**: NEW (informational — likely intentional)
- **Description**: Several `let _unused = r.u16_or_default();` reads advance the
  stream cursor past fields the parser does not consume. These are
  **stream-position-correct** (consuming bytes to stay aligned), not refactor
  cruft — but CLAUDE.md prefers not binding to `_var`.
- **Evidence**: Each call advances the reader; removing the binding would change
  parse position. The pattern is deliberate cursor advancement.
- **Impact**: None functionally; minor naming-convention drift.
- **Suggested Fix**: Where `u16_or_default()` is purely cursor advancement,
  prefer a `skip(N)`/`advance(N)` helper over a discarded read so intent reads as
  "consume bytes", not "read then drop". Defense-in-depth only; do **not**
  delete the reads (they hold stream alignment).

---

## 6. Verified-Holding (no finding — recorded so the next audit can diff)

- **GPU struct doc sizes correct**: `GpuCamera` 336 B, `GpuInstance` 112 B,
  `GpuMaterial` 300 B — all match pinned tests
  (`gpu_instance_layout_tests.rs:34,59`, `material.rs:1202`). Doc claims in
  `context/mod.rs:677` (336 B) and `docs/engine/shader-pipeline.md:105/124/157`
  match.
- **`Material::classify_pbr` doc-rot NOT present**: every doc comment in
  `crates/core/src/ecs/components/material.rs` referencing `classify_pbr`
  correctly frames it as deleted/historical; the surviving symbols
  (`classify_pbr_keyword`, `Material::resolve_pbr`) are named correctly.
- **Last cycle's TD3-001 fixed**: no stale 304-B GpuCamera claim survives in
  `context/mod.rs`.
- **Shader `#define` provenance clean**: generated
  `shaders/include/shader_constants.glsl` is the source; no inline literal
  bypasses it.
- **`read_zstring` single production home**: one in
  `crates/plugin/src/esm/records/common.rs`; the two copies in
  `crates/plugin/examples/` are standalone dev tools (not workspace consumers) —
  not a consolidation target.
- **Path-validation gate clean**: 979 refs across 26 skill files, 0 stale.
- **New crates clean**: `crates/pex/`, `crates/save/`, expanded
  `crates/scripting/` — 0 markers, 0 `#[allow(dead_code)]`, 0 `unimplemented!`,
  no file >2000 LOC.
- **No genuine `#[ignore]` debt**: all 261 ignores gate Vulkan/GPU/on-disk-data
  or `--ignored` golden frames.

## 7. Deferred (gated on in-progress milestones — not debt yet)

- **M47.1 condition stubs** (`crates/scripting/src/condition.rs`): GetActorValue,
  GetDistance, GetFactionRank, GetIsID, HasPerk return safe defaults pending
  their backing components/resolvers. Tracked under OPEN #1316 (TD5-NEW-01) +
  #1663–#1668. These are *deferred feature work with live issues*, not stale debt.

---

## 8. Cross-Dimension Dedup Notes

- The `material.rs:608` glass TODO reported once under Dim 5 (TD5-001), deduped
  to OPEN #1627 — not also under Dim 3.
- File-growth reported under Dim 1; the underlying split axes are the OPEN
  #1669–#1673 / #1323 issues — restated with refreshed LOC, not re-filed.
- Material doc-rot would report under Dim 3 if present — verified *not* present.
- NIFAL/material translation *correctness* is out of scope (owned by
  `/audit-nifal`); only the surrounding doc/dead-code debt was checked.
