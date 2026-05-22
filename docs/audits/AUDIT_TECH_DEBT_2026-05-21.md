# Tech-Debt Audit — 2026-05-21

**Scope**: 10 dimensions, deep depth.
**Prior baseline**: [AUDIT_TECH_DEBT_2026-05-19.md](AUDIT_TECH_DEBT_2026-05-19.md).
**Method**: In-thread breadth-first grep + `_audit-validate.sh` gate (per 2026-05-19 methodology note — fanout pattern remains the wrong shape for this audit).

---

## Executive Summary

**1 NEW finding + 3 carryovers** across 10 dimensions. The 2026-05-19 quick wins (`TD4-NEW-01` shader flag constants, `TD7-NEW-01` / `TD10-NEW-01` `render.rs` path rot) all shipped. A new path-rot finding from the [a5aa5768](https://github.com/matiaszanolli/ByroRedux/commit/a5aa5768) `tri_shape.rs` split takes their place — same class, same trivial fix.

| Severity | New | Carryover | Total | Dimensions |
|----------|-----|-----------|-------|-----------|
| MEDIUM   | 0   | 2         | 2     | D10 (1 — #1156, unchanged), D9 (1 — #1118, BLOCKED, grew +291 LOC) |
| LOW      | 1   | 2         | 3     | D7/D10 (1 NEW — 4 stale `tri_shape.rs` refs), D4 (2 carry — TD4-201, TD4-202) |
| INFO/PASS | — | —         | —     | D1 / D2 / D3 / D5 / D6 / D8 — verified clean or known-deferred |

**Baseline counts (delta vs 2026-05-19)**:

| Metric | 2026-05-19 | 2026-05-21 | Δ |
|--------|-----------:|-----------:|--:|
| `TODO/FIXME/HACK/XXX` | 4 | 4 | 0 |
| `#[allow(dead_code)]` | 25 | 26 | +1 (`cell_loader/refr.rs:64` — XTXR slot 6 `inner` parity placeholder for MultiLayerParallax round-trip; justified) |
| `unimplemented!()` / `todo!()` | 0 | 0 | 0 |
| `#[ignore]` tests (scoped to `crates/` + `byroredux/`) | 111 | 112 | +1 (`crates/plugin/tests/parse_real_esm.rs`, install-data gated) |
| Files >2000 LOC | 2 | 2 | 0 count; **+291 LOC** across the two BLOCKED files |
| Stale local `.claude/issues/<N>/ISSUE.md` (inclusive regex) | ~164 | ~164 | 0 (unchanged; #1156 still open) |

**Wins shipped since 2026-05-19**:

- **TD4-NEW-01** closed — `INSTANCE_FLAG_NON_UNIFORM_SCALE` / `_ALPHA_BLEND` / `_TERRAIN_SPLAT` / `_FLAT_SHADING` constants now drive [triangle.vert:174](../../crates/renderer/shaders/triangle.vert#L174) + [triangle.frag:800-1476](../../crates/renderer/shaders/triangle.frag#L800-L1476). `MAT_FLAG_EFFECT_*` family followed the same conversion at [triangle.frag:1172-1206](../../crates/renderer/shaders/triangle.frag#L1172-L1206). The bare-literal drift surface yesterday's report flagged is gone.
- **TD7-NEW-01 / TD10-NEW-01** closed — 12 stale `byroredux/src/render.rs` refs swept from the 5 audit skill files (no longer in the gate's output).
- **#1118 TD9-005** ([a5aa5768](https://github.com/matiaszanolli/ByroRedux/commit/a5aa5768)) split `crates/nif/src/blocks/tri_shape.rs` (1875 LOC) into `tri_shape/{ni_tri_shape,bs_tri_shape,agd}.rs` siblings. Closes one of the long-tail Session-34/35 splits — same pattern as `acceleration/`, `scene_buffer/`, `collision/`, `mesh/`.

That's the entire trivial-effort cleanup queue from 2026-05-19. The only carryovers now are the two **BLOCKED** medium investments and the two batch-sized magic-number sweeps (TD4-201, TD4-202).

---

## Baseline Snapshot

```
Date: 2026-05-21
TODO/FIXME/HACK/XXX: 4
allow(dead_code): 26
unimplemented!/todo!(): 0
#[ignore] tests (scoped): 112
files >2000 LOC: 2
  - crates/renderer/src/vulkan/context/draw.rs (2899)  — was 2736 on 2026-05-19 (+163)
  - crates/renderer/src/vulkan/context/mod.rs  (2661)  — was 2533 on 2026-05-19 (+128)
stale local ISSUE.md (inclusive regex): ~164
```

The two BLOCKED files added **+291 LOC** in 2 days. Sources:
- [Fix #890 Stage 2c](https://github.com/matiaszanolli/ByroRedux/commit/7eb137b5) — `BSEffectShaderProperty` greyscale-to-palette LUT shader consumer
- [Fix #1194](https://github.com/matiaszanolli/ByroRedux/commit/e5774b19) (PERF-DIM7-INSTR) — per-pass GPU timer + dispatches_skipped counter

Both are legitimate functional adds, but the trajectory (2487 → 2533 → 2661 for `mod.rs`, 2656 → 2736 → 2899 for `draw.rs`) means the BLOCKED status is starting to cost real lines. Same recommendation as 2026-05-19: gate movement on RenderDoc-baseline + integration-test infra, not in-thread speculation (per [`feedback_speculative_vulkan_fixes.md`](../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_speculative_vulkan_fixes.md)).

---

## Top Quick Win

Only one this cycle:

1. **TD7-NEW-02 / TD10-NEW-02** *(same root cause)* — Replace 4 backticked refs to `crates/nif/src/blocks/tri_shape.rs` with the post-split paths. `_audit-validate.sh` flags all 4. The fix is mechanical sed across the 4 files and the gate then locks it in. Effort: trivial.

---

## Findings — Grouped by Severity

### MEDIUM (carryover only)

#### TD10-001 *(carryover, #1156)* — Stale local `.claude/issues/<N>/ISSUE.md` files marked OPEN while GitHub shows CLOSED

- **Severity**: MEDIUM (systemic operational-record drift)
- **Dimension**: Audit-Finding Rot
- **Status today**: Unchanged. ~164 stale files; #1156 still open with three options on the table.
- **Recommendation still C** — declare local `.claude/issues/` files as immutable creation snapshots in `_audit-common.md`. One paragraph. Once documented, the dimension stops re-flagging this every audit.
- **Effort**: trivial (one paragraph) if the workflow decision lands; medium otherwise.

#### TD9-200 / TD9-201 *(carryover, BLOCKED, #1118)* — `context/draw.rs` (2899) + `context/mod.rs` (2661) over the 2000-LOC ceiling

- **Severity**: MEDIUM (BLOCKED on [`feedback_speculative_vulkan_fixes.md`](../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_speculative_vulkan_fixes.md))
- **Dimension**: File / Function Complexity
- **Delta vs 2026-05-19**: `draw.rs` +163 LOC (2736 → 2899); `mod.rs` +128 LOC (2533 → 2661). Total +291 LOC in 2 days, double the previous 2-day delta. Driven by #890 Stage 2c + #1194 instrumentation — both legitimate.
- **Trajectory**: `mod.rs` is on track to cross 2700 LOC next week; `draw.rs` already at ~3K. Each functional add is small in isolation; the integral isn't.
- **Why still BLOCKED**: Vulkan render-pass / pipeline / command-recording splits have failure modes invisible to `cargo test`. RenderDoc baseline + integration test infra is the gate. No movement on that gate this week.
- **Unblock candidate**: Worth considering whether the M28.5 collide-and-slide work or the M40 cell-swap orchestration give us enough of a "running cell" smoke test to start treating per-pass diff captures as regression evidence. Not a recommendation — a thread to pull next time the BLOCKED status is reviewed.

---

### LOW (1 new, 2 carryover)

#### TD7-NEW-02 / TD10-NEW-02 — 4 stale `crates/nif/src/blocks/tri_shape.rs` refs in 4 audit skill files post-#1118 split

- **Severity**: LOW (could be MEDIUM under "stale doc baseline that misled an audit in the last 90 days" — the gate catches them, but the prompt prose still names the stale path)
- **Dimension**: Stale Documentation / Audit-Finding Rot (same root cause; deduped)
- **Locations** (all flagged by [`.claude/commands/_audit-validate.sh`](../../.claude/commands/_audit-validate.sh)):
  - [audit-fo4.md:50](../../.claude/commands/audit-fo4.md#L50) — "BSTriShape parser folded into the unified file post-Session-35"
  - [audit-renderer.md:274](../../.claude/commands/audit-renderer.md#L274) — "VF_TANGENTS = 0x010, packed-vertex tangent stride"
  - [audit-skyrim.md:56](../../.claude/commands/audit-skyrim.md#L56) — same prose as audit-fo4.md:50
  - [audit-starfield.md:68](../../.claude/commands/audit-starfield.md#L68) — same prose
- **Age**: 1 day. Introduced by [#1118 TD9-005](https://github.com/matiaszanolli/ByroRedux/commit/a5aa5768) (2026-05-20) — `tri_shape.rs` (1875 LOC) → `tri_shape/{ni_tri_shape,bs_tri_shape,agd}.rs`.
- **Reality**: `crates/nif/src/blocks/tri_shape.rs` no longer exists. The directory is:
  - [`crates/nif/src/blocks/tri_shape/ni_tri_shape.rs`](../../crates/nif/src/blocks/tri_shape/ni_tri_shape.rs) — classic NiTriShape parser
  - [`crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`](../../crates/nif/src/blocks/tri_shape/bs_tri_shape.rs) — Skyrim SE+ packed-half BSTriShape parser (the file the FO4 / Skyrim / Starfield checklists actually need)
  - [`crates/nif/src/blocks/tri_shape/agd.rs`](../../crates/nif/src/blocks/tri_shape/agd.rs) — NiAdditionalGeometryData
- **Suggested replacement**:
  - `audit-fo4.md:50`, `audit-skyrim.md:56`, `audit-starfield.md:68` — replace `crates/nif/src/blocks/tri_shape.rs` with `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` (that's where the BSTriShape parser actually lives). Update the trailing "folded into the unified file post-Session-35" prose to "split out into `tri_shape/bs_tri_shape.rs` post-#1118 (2026-05-20)".
  - `audit-renderer.md:274` — same swap for the "VF_TANGENTS = 0x010, packed-vertex tangent stride" pointer. The `:277` line numbers (665-730 in the old file) need to be re-anchored; the packed-vertex loop is now in the smaller [`bs_tri_shape.rs`](../../crates/nif/src/blocks/tri_shape/bs_tri_shape.rs) file — switch to symbol-based anchors (`BSTriShape` parser, packed-vertex loop) per the post-#1040 convention.
- **Effort**: trivial — single sed sweep across 4 files, then re-run the gate.

#### TD4-201 *(carryover)* — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants

- **Severity**: LOW
- **Dimension**: Magic Numbers
- **Status today**: Sample re-grepped. Still present at `crates/nif/src/header.rs:404`, `:467`, `:565`, `:590` (test fixture writers using `0x14020007`, `0x14010000`, `0x14020005` literals) and across various block dispatch tests. Mechanical work spread across 12+ files; needs a batch slot. No movement.

#### TD4-202 *(carryover)* — 112 ESM subrecord size literals (`if data.len() == N`) should map to named `RecordType::*_SIZE` constants

- **Severity**: LOW
- **Dimension**: Magic Numbers
- **Status today**: Count unchanged at 112 (was reported as 142 in 2026-05-19; the actual count for the strict `data.len() == N | >= N | < N` pattern is 112 — the 142 figure included size comparisons across all `len()` predicates). Per-record-type independent; could parallelize. No movement.

---

## PASS Dimensions

### Dim 1 — Stale Markers: PASS

All 4 markers are commentary, not active rot. Same set as 2026-05-19:
- [`crates/bgsm/src/bgem.rs:122`](../../crates/bgsm/src/bgem.rs#L122) — references upstream-reference FIXME (not actionable here)
- [`crates/nif/src/blocks/bs_geometry.rs:552`](../../crates/nif/src/blocks/bs_geometry.rs#L552) — same upstream FIXME cross-ref
- [`byroredux/src/scene.rs:585`](../../byroredux/src/scene.rs#L585) — "Closes the #242 consumer-side TODO (#1055)" commentary on a CLOSED issue
- [`byroredux/src/main.rs:1390`](../../byroredux/src/main.rs#L1390) — same #242 closure commentary

### Dim 2 — Dead Code: PASS

26 `#[allow(dead_code)]` instances; +1 vs 2026-05-19. The new one ([`byroredux/src/cell_loader/refr.rs:64`](../../byroredux/src/cell_loader/refr.rs#L64)) is the `inner` field on `RefrTextureOverlay` — slot 6 of the XTXR overlay (MultiLayerParallax inner layer), preserved for round-trip parity with `TextureSet.inner` so the slot_index=6 XTXR swap works. Same staged-rollout pattern as the existing `CellLightingRes` markers in [`components.rs`](../../byroredux/src/components.rs#L227-L264). Justified; no debt.

### Dim 3 — Logic Duplication: PASS

Reverified the `EXTERIOR_CELL_UNITS = 4096.0` consolidation from [TD3-202 / #1112](../../crates/core/src/math/coord.rs#L41) — all remaining `4096.0` occurrences are either (a) test assertions on the constant itself, (b) per-feature `fog_far` / `light_radius_or_default` defaults that legitimately coincide with the cell width, or (c) the streaming-tests `world_pos_to_grid(4096.01, 0.0)` boundary check. No new duplication patterns from #890 / #1194 / #1212-#1224 / M28.5 / M40 deltas.

### Dim 4 — Magic Numbers: 2 carryover only (above)

TD4-NEW-01 (INSTANCE_FLAG / MAT_FLAG shader constants) closed. Only TD4-201 / TD4-202 remain.

### Dim 5 — Stub Implementations: PASS

Zero `unimplemented!()`, `todo!()`, `panic!("not implemented")` in `crates/` + `byroredux/`. Clean since 2026-05-17.

### Dim 6 — Test Hygiene: PASS

+1 `#[ignore]` test since 2026-05-19 (112 vs 111). Install-data gated; same justified pattern as the prior +6. Golden-frame coverage unchanged. No commented-out asserts surfaced in spot checks.

### Dim 8 — Backwards-Compat Cruft: PASS

Zero `// removed:` comments. Zero `#[deprecated]` items. M28.5 + M40 work landed without leaving compatibility-shim breadcrumbs.

### Dim 9 — File / Function Complexity: 1 carryover (above)

Only `context/draw.rs` and `context/mod.rs` over the 2000-LOC ceiling. Next-largest is `byroredux/src/main.rs` at 1920 LOC — climbing but not yet over. `asset_provider.rs` at 1820 and `import/tests.rs` at 1788 are also worth watching but not findings yet.

### Dim 10 — Audit-Finding Rot: 2 carryover + 1 NEW (above)

`_audit-validate.sh` is doing its job: it caught the new `tri_shape.rs` rot the moment #1118 landed, before any audit could be misled.

---

## Top 5 Medium Investments

Identical to 2026-05-19, in priority order:

1. **TD9-200 / TD9-201** — `context/draw.rs` (2899) + `context/mod.rs` (2661) splits. **BLOCKED** on RenderDoc baseline / integration tests. The +291-LOC-in-2-days trajectory is the new data point.
2. **TD10-001** — 164 stale local ISSUE.md files. Recommend documenting the immutable-snapshot semantics (Option C in #1156) and stopping the relitigation.
3. **TD4-201** — 32 bare-hex NIF version compares.
4. **TD4-202** — 112 ESM subrecord size literals.
5. *(slot empty this cycle — TD4-NEW-01 closed)*

---

## Deferred (gated by milestones)

- FO4 BGSM Phase 2b consumer items continue to land incrementally (#890 Stage 2c shipped this week; #1147 still ongoing).
- M28.5 character-controller work just landed; any related tech-debt should wait one bench cycle before being flagged.
- M40 cell-swap orchestration ([a7cc9184](https://github.com/matiaszanolli/ByroRedux/commit/a7cc9184), [1e92a471](https://github.com/matiaszanolli/ByroRedux/commit/1e92a471), [f6b9911a](https://github.com/matiaszanolli/ByroRedux/commit/f6b9911a)) added DoorTeleport + cell_for_refr reverse-lookup — same caveat.

---

## Notes for Next Audit

- **Methodology**: In-thread breadth-first remains the right shape. This audit ran in ~12 tool calls without exhausting any budget. The 2026-05-19 methodology recommendation stands.
- **Validate gate uptake**: 2 audits running in a row now where `_audit-validate.sh` caught the path-rot finding *before* the audit prose did. The gate is the structural fix; treat its STALE output as the canonical "Dim 7 + Dim 10 path-rot" input rather than re-grepping for renamed files.
- **BLOCKED Vulkan splits**: At +291 LOC over 2 days, the trajectory argues for adding "design a RenderDoc-driven smoke test" to the M28.5 / M40 follow-up work — not as a new dim finding (it's a roadmap item), but as a precondition for unblocking TD9-200/201.
- **TD3-202 closeout** (4096.0 → EXTERIOR_CELL_UNITS) is a good example of how to do these consolidations: one named constant in [`crates/core/src/math/coord.rs`](../../crates/core/src/math/coord.rs#L41) with a regression test pinning the value, plus a doc comment naming the original scattered sites. Future TD3-* fixes should follow the same pattern.
