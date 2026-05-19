# Tech-Debt Audit — 2026-05-19

**Scope**: 10 dimensions, deep depth.
**Prior baselines**: `AUDIT_TECH_DEBT_2026-05-13.md` / `-14.md` / `-16.md` / `-17.md`.
**Method note**: Re-ran in-thread after `/audit-renderer` showed the orchestrator-with-deep-checklists agent pattern exhausts tool budgets before the writeup phase. Tech-debt audits are breadth-first grep/wc/git work — they fit a single thread better than the 10-agent fanout the skill prescribes.

---

## Executive Summary

**2 NEW findings + 4 carryovers** across 10 dimensions. Most of the 2026-05-17 quick-win batch shipped between yesterday and today.

| Severity | New | Carryover | Total | Dimensions |
|----------|-----|-----------|-------|-----------|
| MEDIUM   | 0   | 2         | 2     | D10 (1 — #1156, grown), D9 (1 — #1118, BLOCKED, slightly grown) |
| LOW      | 2   | 2         | 4     | D7/D10 (1 NEW — 12 stale render.rs refs), D4 (1 NEW — shader bare-literal `inst.flags`), D4 (2 carry) |
| INFO/PASS | — | —         | —     | D1 / D2 / D3 / D5 / D6 / D8 — verified clean or known-deferred |

**Baseline counts (delta vs 2026-05-17)**:

| Metric | 2026-05-17 | 2026-05-19 | Δ |
|--------|-----------:|-----------:|--:|
| `TODO/FIXME/HACK/XXX` | 4 | 4 | 0 |
| `#[allow(dead_code)]` | 25 | 25 | 0 |
| `unimplemented!()` / `todo!()` | 0 | 0 | 0 |
| `#[ignore]` tests (scoped to `crates/` + `byroredux/`) | 105 | 111 | +6 (all install-data-gated, justified) |
| Files >2000 LOC | 2 | 2 | 0 count; +126 LOC across the two BLOCKED files |
| Stale local `.claude/issues/<N>/ISSUE.md` (inclusive regex) | 80\* | ~164 | +84 (#1156 still open) |

\* The 2026-05-17 count used a stricter regex (`State: OPEN` only). Today's re-measurement (`(state|status)[:*]\s*(open|new)` case-insensitive across the first 30 lines of each file) returns 164/1059 files. The growth-from-80 is partly methodology, partly real drift — no automation has run between the two audits to close the gap.

**Wins since 2026-05-17** (shipped to main):

- **TD3-207** closed via #1149 — 14 inline `ImageSubresourceRange` literals migrated to `color_subresource_single_mip()`
- **TD4-301 / TD7-052** closed via #1150 — `skin_compute.rs` MAX_BONES math comment fixed
- **TD4-302** closed via #1151 — `cluster_cull.comp` redundant `THREADS_PER_CLUSTER` redeclaration deleted
- **TD4-303** closed via #1152 — 4 compute shaders now use `WORKGROUP_X/Y` instead of hardcoded 8
- **TD7-051** closed via #1150 — CLAUDE.md "25 floats" claim corrected
- **TD8-023** closed via #1120 — `TreeObjectBounds` dead re-export deleted
- **TD8-024** closed via b3096bd3 — `SKINNED_BLAS_FLAGS` doc comment no longer cites deleted function
- **TD2-201..204** closed via #1120 — 4 dead-code cleanups
- Plus the **REN-D6-NEW-01** (`BLOOM_INTENSITY` / `VOLUME_FAR` shader-side const redeclarations) closed via #1126 — same pattern class as TD4-302/303

8 of the 10 quick wins from yesterday's report shipped within 48 hours. The two that didn't (TD4-201 bare-hex NIF version compares, TD4-202 ESM subrecord size literals) are mechanical-but-volume work (32 sites + 142 sites respectively) — they need a batch slot, not a quick-win slot.

---

## Baseline Snapshot

```
Date: 2026-05-19
TODO/FIXME/HACK/XXX: 4
allow(dead_code): 25
unimplemented!/todo!(): 0
#[ignore] tests (scoped): 111
files >2000 LOC: 2
  - crates/renderer/src/vulkan/context/draw.rs (2736) — was 2656 on 2026-05-17 (+80)
  - crates/renderer/src/vulkan/context/mod.rs (2533) — was 2487 on 2026-05-17 (+46)
stale local ISSUE.md (inclusive regex): ~164
```

The two BLOCKED files grew by a combined +126 LOC over 2 days, with the bulk coming from #869 (NiWireframeProperty `LINE` pipeline variant + flat_shading consumer) — both legitimate functional adds, but the trend reinforces why #1118 / TD9-200 should not be deferred indefinitely. See Dim 9 below.

---

## Top 2 Quick Wins

Yesterday's batch closed nearly every trivial-effort item. Today's are smaller and the new ones are mechanical:

1. **TD7-NEW-01 / TD10-NEW-01** *(same root cause)* — Replace 12 backticked refs to `byroredux/src/render.rs` with `byroredux/src/render/mod.rs` (or just `byroredux/src/render/`) across 5 audit skill files. The `_audit-validate.sh` gate (#1114) catches these every time it runs; today's `/audit-renderer` run actually surfaced these as the gate's only failures. Effort: trivial (sed across 5 files), and the gate locks the fix in place. See finding below.

2. **TD4-NEW-01** — Add `INSTANCE_FLAG_NON_UNIFORM_SCALE`, `_ALPHA_BLEND`, `_TERRAIN_SPLAT`, `_PRESKINNED`, `_FLAT_SHADING` constants to `crates/renderer/src/shader_constants_data.rs` so the generated `shader_constants.glsl` carries them; then replace the bare numeric literals (`(inst.flags & 1u)`, `& 2u`, `& 8u`, `& 128u`) at the 8+ shader sites. Effort: small (~20 min); pins the Rust↔shader contract the same way `MAX_BONES_PER_MESH` and `WORKGROUP_X` are pinned today.

---

## Findings — Grouped by Severity

### MEDIUM (carryover only)

#### TD10-001 *(carryover, #1156)* — Stale local `.claude/issues/<N>/ISSUE.md` files marked OPEN while GitHub shows CLOSED

- **Severity**: MEDIUM (systemic operational-record drift)
- **Dimension**: Audit-Finding Rot
- **Status today**: Sample of 20 random files marked locally `OPEN`/`NEW` returned **20/20 closed on GitHub** — 100% drift in the sample. Total stale ~164/1059 = 15% of all local issue files.
- **Delta vs 2026-05-17**: The original count of 80 used a stricter regex; today's inclusive count is 164. No automation runs between audits to close the gap, so the gap grows monotonically.
- **Decision still pending** (per #1156): A / sync hook on `gh issue close`, B / deprecate local Status field, or C / treat `.claude/issues/` as immutable creation snapshots. **Recommend C** — the local files were always meant to be the launch payload of `/fix-issue`, not a live mirror. Once C is documented, today's 164 stale files become "by-design archival" and the audit dimension can stop re-flagging them.
- **Effort**: trivial (one paragraph in `_audit-common.md` documenting the C semantics) if the workflow decision is made. Currently medium (per-file mass-update) without one.

#### TD9-200 / TD9-201 *(carryover, BLOCKED, #1118)* — `context/draw.rs` (2736) + `context/mod.rs` (2533) over the 2000-LOC ceiling

- **Severity**: MEDIUM (BLOCKED on `feedback_speculative_vulkan_fixes.md`)
- **Dimension**: File / Function Complexity
- **Delta vs 2026-05-17**: `draw.rs` +80 LOC (2656 → 2736); `mod.rs` +46 LOC (2487 → 2533). Both increments came from legitimate functional adds (#869 wireframe pipeline + flat_shading consumer wiring, #952 fence-reset reorder + recovery helper, #1188 today's `images_in_flight` invalidation, #1136 FX-marker hoist).
- **Why still BLOCKED**: Vulkan render-pass / pipeline / command-recording splits have failure modes that don't surface in `cargo test` (per `feedback_speculative_vulkan_fixes.md`). A RenderDoc baseline + integration test infrastructure is the gate. No movement on that gate this week.
- **Mitigation**: today's split work landed in `byroredux/src/render/{sky,lights,water,particles,camera,skinned,static_meshes}.rs` (#1115 Steps 1-8) — that's the **caller** side of the Vulkan boundary, not the per-frame recording side. The renderer-side splits remain locked.

---

### LOW (2 new, 2 carryover)

#### TD7-NEW-01 / TD10-NEW-01 — 12 stale `byroredux/src/render.rs` refs in 5 audit skill files post-#1115 refactor

- **Severity**: LOW (could be MEDIUM under "stale doc baseline that has misled an audit in the last 90 days" — and it did mislead today's `/audit-renderer` run; the gate caught it but the prompt itself still names the stale path)
- **Dimension**: Stale Documentation / Audit-Finding Rot (same root cause; deduped here)
- **Locations**: All flagged by `.claude/commands/_audit-validate.sh`:
  - `.claude/commands/audit-fo3.md:66`
  - `.claude/commands/audit-incremental.md:43`
  - `.claude/commands/audit-performance.md:51`, `:77`, `:87`
  - `.claude/commands/audit-renderer.md:194`, `:211`, `:220`, `:241`, `:258`, `:333`
  - `.claude/commands/audit-safety.md:52`
- **Age**: 4 days. Introduced by the #1115 Step-1 refactor (`1164917d`, 2026-05-15) that promoted `byroredux/src/render.rs` → `byroredux/src/render/mod.rs`. The 8 follow-up steps extracted submodules but the file move was atomic at step 1.
- **Reality**: `byroredux/src/render.rs` no longer exists; the index is `byroredux/src/render/mod.rs`, and most logic lives in `byroredux/src/render/{static_meshes, lights, sky, water, particles, camera, skinned}.rs`.
- **Why this matters**: When `/audit-renderer` runs the dim-prompt "Entry points: `byroredux/src/render.rs`", an agent that doesn't trust the validate gate's output will fail to find the file and spend tool budget chasing the rename through `git log` — exactly what happened in one of today's failed dim agents.
- **Effort**: trivial — sed across 5 files. Two reasonable replacements:
  - `byroredux/src/render.rs` → `byroredux/src/render/mod.rs` (when the ref points to top-level dispatch)
  - `byroredux/src/render.rs` → `byroredux/src/render/` (when the ref means "the rendering module as a whole")
- **Suggested process**: A single sweep commit that updates all 12 sites, then re-runs the gate to confirm. The validate gate (#1114) is the structural fix; this is just the cleanup it asks for.

#### TD4-NEW-01 — `inst.flags` bare numeric literals across 8+ shader sites; only 1 of 5 bits is test-pinned

- **Severity**: LOW
- **Dimension**: Magic Numbers (lockstep drift surface)
- **Locations**:
  - `crates/renderer/shaders/triangle.vert:174` — `(inst.flags & 1u)`
  - `crates/renderer/shaders/triangle.frag:22` (comment), `:793`, `:856`, `:901`, `:995`, `:1427` — `(inst.flags & {1u,2u,8u,128u})`
  - Plus the `INSTANCE_FLAG_TERRAIN_SPLAT` comment-only ref at `:261`
- **Reality**: `crates/renderer/src/vulkan/scene_buffer/constants.rs` declares the canonical bits:
  - `INSTANCE_FLAG_NON_UNIFORM_SCALE = 1 << 0`
  - `INSTANCE_FLAG_ALPHA_BLEND = 1 << 1`
  - `INSTANCE_FLAG_CAUSTIC_SOURCE = 1 << 2`
  - `INSTANCE_FLAG_TERRAIN_SPLAT = 1 << 3`
  - `INSTANCE_FLAG_PRESKINNED = 1 << 6`
  - `INSTANCE_FLAG_FLAT_SHADING = 1 << 7`
- **Pin status**: Only `INSTANCE_FLAG_FLAT_SHADING` has a Rust-side test pinning the shader literal (`flat_shading_bit_pinned_at_128_for_shader_constant`, [scene_buffer/constants.rs:253](../../crates/renderer/src/vulkan/scene_buffer/constants.rs#L253)). The other 5 bits drift silently if either side changes.
- **Why this matters**: Same hazard class as `feedback_shader_struct_sync.md`. Today the bits happen to align, but the next bit-reshuffling (e.g., when terrain-tile-window bits 16-31 are eventually compressed) becomes a silent-corruption risk.
- **Effort**: small (~20 min)
  1. Add `pub const INSTANCE_FLAG_*` to `crates/renderer/src/shader_constants_data.rs`
  2. Re-run `cargo build -p byroredux-renderer` (regenerates `shader_constants.glsl`)
  3. Replace `& 1u` / `& 2u` / `& 8u` / `& 128u` at the 6+ sites with `& INSTANCE_FLAG_*` (uppercase, post-include)
  4. Recompile SPIR-V for `triangle.vert` + `triangle.frag`
  5. The `flat_shading_bit_pinned_at_128_for_shader_constant` test becomes redundant once the include is the canonical source — replace with a single Rust↔include round-trip test if not already present
- **Sibling check**: `MAT_FLAG_*` constants in `triangle.frag:141` and `:1136-1138` use the same hardcoded-literal pattern (`0x1u`, `0x2u`, `0x4u`, `0x8u`, `0x10u` for VERTEX_COLOR_EMISSIVE / EFFECT_SOFT / EFFECT_PALETTE_COLOR / EFFECT_PALETTE_ALPHA / EFFECT_LIT) — same fix applies. Including these in the same sweep would pin both flag families.

#### TD4-201 *(carryover)* — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants

- **Severity**: LOW
- **Dimension**: Magic Numbers
- **Status**: No movement since 2026-05-17. Mechanical work spread across 12+ block files; needs a batch slot.

#### TD4-202 *(carryover)* — 142 ESM subrecord size literals (`if data.len() == 24`) should map to named `RecordType::*_SIZE` constants

- **Severity**: LOW
- **Dimension**: Magic Numbers
- **Status**: No movement since 2026-05-17. Per-record-type independent — could parallelize via a batched fix-issue run.

---

## PASS Dimensions

### Dim 1 — Stale Markers: PASS

All 4 markers are commentary on closed issues, not active rot:
- `crates/bgsm/src/bgem.rs:122` — references a FIXME in the upstream reference repo (not actionable here)
- `crates/nif/src/blocks/bs_geometry.rs:552` — references the same upstream FIXME (doc cross-ref)
- `byroredux/src/scene.rs:474` — comment says "Closes the #242 consumer-side TODO (#1055)" (closed, just commentary)
- `byroredux/src/main.rs:1149` — same #242 closure commentary

Same state as 2026-05-17. No change needed.

### Dim 2 — Dead Code: PASS

All 25 `#[allow(dead_code)]` instances verified justified (CellLightingRes staged-rollout markers in `components.rs`, BA2 mip fields in `ba2.rs`, NIF schema constants in `tri_shape.rs`, test helpers in `nif/tests/common`, etc.). Same baseline as 2026-05-17.

### Dim 3 — Logic Duplication: PASS (modulo carryovers)

TD3-207 (inline `ImageSubresourceRange`) closed via #1149. No new duplication patterns from the #869 / #1115 / #1147 deltas. Render-side split (#1115) introduced clean module boundaries (`render/{sky,lights,water,particles,camera,skinned,static_meshes}.rs`) rather than copy-paste.

### Dim 5 — Stub Implementations: PASS

Zero `unimplemented!()`, `todo!()`, or `panic!("not implemented")` in `crates/` + `byroredux/`. Holding at clean since 2026-05-17.

### Dim 6 — Test Hygiene: PASS

+6 `#[ignore]` tests since 2026-05-17 — all install-data-gated (Oblivion CLAS / RACE smokes, Starfield materialsbeta.cdb smoke). Standard justified pattern: data not present in CI, gated locally. No commented-out asserts found in sampled test files.

### Dim 8 — Backwards-Compat Cruft: PASS

Zero `// removed:` comments. Zero `#[deprecated]` items. The `_dt: f32` / `_world: &World` parameter prefixes in `audio_system`, console traits, etc., are trait-required signatures with unused params — standard Rust pattern, not cruft.

---

## Top 5 Medium Investments (carryover)

Same as 2026-05-17, no new entries:

1. **TD9-200 / TD9-201** — `context/draw.rs` + `context/mod.rs` splits. **BLOCKED** until RenderDoc baseline / integration tests land. The +126 LOC delta in 2 days underlines the urgency of unblocking, but `feedback_speculative_vulkan_fixes.md` policy stands.
2. **TD10-001** — 164 stale local ISSUE.md files. Recommend documenting the immutable-snapshot semantics (Option C) rather than trying to keep them synced.
3. **TD4-201** — 32 bare-hex NIF version compares.
4. **TD4-202** — 142 ESM subrecord size literals.
5. **TD4-NEW-01** (new today) — INSTANCE_FLAG_* / MAT_FLAG_* shader constants drift surface.

---

## Deferred (gated by milestones)

- All FO4 BGSM Phase 2b items (#1147) are gated by the shader-side branch landing; today's Phase 2a only plumbs the host bits.
- Volumetric fog re-engagement (#924 closed today) needs its bench-cycle verification before any related tech-debt items are flagged.

---

## Notes for Next Audit

- **Methodology**: The orchestrator-with-deep-checklists agent pattern failed twice today (`/audit-renderer` and the initial fanout attempt for this audit). Tech-debt is the wrong shape for that architecture — it's breadth-first grep work, not depth-first invariant tracing. Consider rewriting `/audit-tech-debt` to be a single agent or in-thread skill rather than a 10-way fanout.
- **Validate gate** (`_audit-validate.sh`) is now the canonical "did anyone touch a referenced path" check. Run it at the START of every audit; pre-2026-05-19 audits should backfill that step into their Phase 1.
- **Local ISSUE.md drift** continues to grow monotonically. Until the workflow decision (#1156) lands, every tech-debt audit will re-discover this finding. Recommend a single sentence in `_audit-common.md` declaring the C / immutable-snapshot semantics so the audits can stop relitigating.
