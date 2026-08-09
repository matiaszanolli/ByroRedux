# #2351: RT-1: bench_draws_batches regressed on skyrim_se (baseline 3 → 8), same symptom class as #2215 but an untracked fourth corpus

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2351  **Labels**: bug, renderer, medium, performance

## Summary

skyrim_se `WhiterunDragonsreach`'s `bench_draws_batches` telemetry has regressed from its baseline of 3 to 8, reproduced identically across two independent back-to-back runs today (`draws=2304/8b/2c` both times). This is the same symptom class as the already-open #2215 (post-batch-merge draw grouping not restoring after `#2165`'s fix) but on a corpus #2215 does not name — #2215 only covers fnv/oblivion/fo4.

## Location

- `byroredux/src/render/mod.rs`, `byroredux/src/render/static_meshes.rs` — draw-batch assembly, touched most recently by `b5d9f181` ("feat(render): add sorting for raster-visible draws and improve draw command handling", 2026-08-01)
- `crates/renderer/src/vulkan/context/draw.rs`, `crates/renderer/src/vulkan/context/geometry_pass.rs` — merge/indirect-grouping consumers

## Description

Baseline `bench_draws_batches` for skyrim_se/WhiterunDragonsreach is 3. The 2026-07-27 sweep measured 9 (not called out as its own finding then, folded into the general delta list). Today's sweep (repo HEAD `1ae86f62`) measured 8, reproduced identically across two independent engine launches — a stable, real behavior change, not small-count sampling noise (contrast with fo3's `gpu_calls` 9↔8↔10 wobble, correctly dismissed as noise at that scale by the 07-27 report).

Confirmed via `gh issue list`: #2215 ("RT-1: #2165's fix does not restore indirect grouping — fnv gpu_calls still 23, oblivion 31, fo4 48 at HEAD") names only fnv/oblivion/fo4. #2216 covers `entities_total`/`skin_pool_live` only. Neither names skyrim_se `bench_draws_batches`.

## Evidence

- Baseline TSV: `.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv` line 16 (`bench_draws_batches 3`)
- This sweep's two independent captures both show `draws=2304/8b/2c` in the `bench:` line

## Impact

Same class of impact as #2215 — the post-merge batch count growing while the cmds count falls (2614→2304 here) means draws that should combine into one indirect batch are not merging, adding avoidable per-frame CPU (sort/group) and GPU (extra indirect submits) overhead. The small absolute magnitude on this cell (3→8) bounds the blast radius today, but if it shares #2215's root cause in `883f57cd`, it will scale with scene complexity like the other three corpora do.

## Related

- #2215 — same symptom class, different corpora (fnv/oblivion/fo4)
- #2216 — this same cell (skyrim_se/WhiterunDragonsreach) also carries the tracked `entities_total`/`skin_pool_live` drift, now escalated (see comment on #2216)

## Suggested Fix

Fold this corpus into #2215's bisection work (`883f57cd` sub-change isolation) rather than treating it as fully disjoint — check whether the same reverted sub-change also restores skyrim_se to ~3 batches. If it does not move together with fnv/oblivion under that bisection, that is itself informative (rules out a single shared cause across all four corpora).

## Completeness Checks

- [ ] **SIBLING**: Check if this affects other games' draw-batching too, or is Skyrim SE-specific (fo3 and fo4 batch counts are currently fine/improved; fnv and oblivion are already tracked under #2215 — confirm whether skyrim_se shares #2215's root cause via the `883f57cd` bisection)
- [ ] **TESTS**: Baseline TSV (`.claude/audit-baselines/runtime/skyrim_se-WhiterunDragonsreach.tsv`) should be updated only once the real cause is understood, not just to hide the regression


---

# #2354: SF-D8-03: NIFAL particle slice is structurally unreachable on Starfield — not documented as N/A

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2354  **Labels**: bug, nif-parser, medium, legacy-compat

**Severity**: MEDIUM
**Dimension**: 8 — NIFAL Canonical Material Translation (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/walk/mod.rs` (`extract_emitter_params`/`extract_emitter_rate`, currently ~line 721 — the audit report's cited 536-563 has drifted with recent edits), `docs/engine/nifal.md`
**Status**: NEW, CONFIRMED against current code

## Description

The NIFAL particle slice (`extract_emitter_params`/`extract_emitter_rate` in `walk/mod.rs`, dispatching on `NiPSysEmitter`/`NiPSys*FieldModifier` blocks) is structurally unreachable on Starfield content — the full Meshes01 per-block histogram (31,058 files, 22 distinct block types) contains zero `NiPSys*`/`NiParticleSystem` blocks. Starfield authors particle systems entirely outside the NIF container (confirmed: Dimension 2's BSGeometry-only import path has no NiNode/NiParticleSystem hierarchy for Starfield content).

This isn't a silent drop of translatable data — it's a structural inapplicability — but nothing in `docs/engine/nifal.md` or the compat matrix states the slice is inapplicable to Starfield, so the NIFAL particle regression suite (#1411/#1434/#1445/#1771/#1775, all Oblivion/FO3/FNV/Skyrim-driven) silently says nothing about Starfield coverage.

## Evidence

- `walk/mod.rs`: `extract_emitter_params`/`extract_emitter_rate` downcast against `NiPSysEmitter`/`NiPSysGravityFieldModifier`/etc. — confirmed present and gated on those block types.
- Dimension 2 of this audit's block-type histogram over the 31,058-file Meshes01 corpus found zero `NiPSys*` blocks.
- `docs/engine/nifal.md` has no "Starfield: particle slice N/A" note.

## Impact

Documentation/tracking gap only — no functional defect. Risk is a future format discovery (e.g. Starfield DLC content authoring particles differently) silently passing the existing test suite instead of flipping a corpus-baseline assertion red.

## Suggested Fix

Record "Starfield: particle slice N/A (particles authored outside the NIF container)" in `docs/engine/nifal.md` and the compat matrix, and add a corpus-baseline assertion (zero `NiPSys*` blocks across the Starfield corpus) so a future format discovery flips a test red instead of passing silently.

## Completeness Checks
- [ ] **SIBLING**: Confirm the same is true for FO76 (also BA2/BGSM/BSGeometry-era) if/when an FO76 audit runs
- [ ] **CANONICAL-BOUNDARY**: This is a documentation-only fix to `docs/engine/nifal.md`; no code path change. See `/audit-nifal`.
- [ ] **TESTS**: Corpus-baseline assertion added (zero `NiPSys*` on Starfield corpus) so future format drift is caught

---

# #2355: SF-D8-04: NIFAL collision slice never fires on Starfield — all colliders route to undecoded BhkSystemBinary, non-Architecture content spawns with no collider at all

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2355  **Labels**: bug, nif-parser, medium, legacy-compat

**Severity**: MEDIUM
**Dimension**: 8 — NIFAL Canonical Material Translation (Starfield audit, 2026-08-03)
**Location**: `byroredux/src/cell_loader/spawn.rs:1477-1478` (synthesized-trimesh fallback, `RenderLayer::Architecture` gate)
**Status**: NEW, CONFIRMED against current code

## Description

The NIFAL collision slice never fires on Starfield content at all — 100% of Starfield colliders route to the undecoded `BhkSystemBinary` blob (33,867 `bhkNPCollisionObject` + 22,895 `bhkPhysicsSystem` + 316 `bhkRagdollSystem` in Meshes01, zero `bhk*Shape` blocks of any kind). `BhkMultiSphereShape`/`BhkConvexListShape` translation, while correctly implemented for Oblivion→FO4, is dead code with respect to Starfield — sharper and broader than the ROADMAP's existing "ragdolls blocked on `BhkSystemBinary`" note, since it's *all* Starfield collision, not just ragdolls.

The synthesized-trimesh fallback is also narrower than the shape arms it stands in for: confirmed it only fires for `RenderLayer::Architecture` (`spawn.rs:1478`: `&& base_layer == byroredux_core::ecs::components::RenderLayer::Architecture`) — so Starfield Clutter/Actor/container content currently spawns with **no collider at all**, not even an approximate one.

## Evidence

- `spawn.rs:1477-1478`: the synthesized-trimesh-collider-ghost fallback is gated on `RenderLayer::Architecture` — confirmed no broader gate exists.
- Corpus histogram (this audit, Dimension 8): zero `bhk*Shape` blocks anywhere in Starfield content; all collision references are `BhkSystemBinary`-backed.

## Impact

All Starfield Clutter/Actor/container content is currently non-collidable (no physics interaction at all), not merely "using an approximate collider." This is a real, measurable rendering/gameplay gap, broader in scope than the existing ROADMAP "ragdolls blocked" note.

## Suggested Fix (short term)

Widen the synthesized-trimesh fallback beyond `RenderLayer::Architecture` to cover Clutter/Actor/container layers too (approximate collision is better than none), and log a once-per-cell count of dropped `BhkSystemBinary` colliders so the gap is measurable going forward.

## Completeness Checks
- [ ] **SIBLING**: Confirm the same `BhkSystemBinary` gap applies to FO76 (same collision family)
- [ ] **CANONICAL-BOUNDARY**: Fix is in `byroredux/src/cell_loader/spawn.rs`, the spawn-time fallback, not the NIFAL translation boundary itself — the real fix (decoding `BhkSystemBinary`) is future work tracked separately in PHYSAL notes. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins the widened fallback (non-Architecture Starfield content gets a synthesized collider)

---

# #2357: SF2D2-03: External .mesh resolve failure is completely silent — the exact #1292 failure mode has no log signal

**State**: OPEN  **URL**: https://github.com/matiaszanolli/ByroRedux/issues/2357  **Labels**: bug, nif-parser, medium, legacy-compat

**Severity**: MEDIUM
**Dimension**: 2 — BSGeometry Mesh Extraction (Starfield audit, 2026-08-03)
**Location**: `crates/nif/src/import/mesh/bs_geometry.rs:66-103`
**Status**: NEW, CONFIRMED against current code

## Description

Stage B (external `.mesh` resolve) has three distinct "no geometry found" exits and **none of them logs anything**:
1. `let resolver = resolver?;` — no resolver supplied, silent early return.
2. The per-slot resolve loop (`resolver.resolve(&canonical)` returning `None`) — archive-resolve miss, no log.
3. `let (tri_size, num_verts, data) = found?;` — every slot exhausted (all resolved but parsed empty/errored), no log at this final point.

Only the rarer sub-failure cases *inside* a successful resolve (parse error, sentinel body) log, and only at `debug!`.

## Evidence

Read `bs_geometry.rs:66-103` directly: confirmed all three exit points return `None` with no `log::` call anywhere on those lines. The sentinel-slot and parse-error sub-cases (lines ~80-95) do log at `debug!`, but that's a different, already-covered failure mode.

## Impact

A future archive-set misconfiguration, missing archive, or path-convention drift reproduces the #1292 symptom (near-total mesh-spawn collapse across all vanilla Starfield content — 288,231 of 320,483 `Meshes01.ba2` entries are `.mesh` companions) with an empty log. Recovering the diagnosis last time (#1292) required a dedicated investigation session.

## Suggested Fix

Add `log::debug!` on the resolve miss (naming the canonical path attempted) and `log::warn!` when every slot is exhausted (naming the shape/mesh name). Consider a dropped-`BSGeometry` counter surfaced via `byro-dbg`.

## Completeness Checks
- [ ] **SIBLING**: Check other external-resource resolve paths (texture resolve, BGSM/BGEM resolve) for the same silent-miss pattern
- [ ] **CANONICAL-BOUNDARY**: Not applicable — this is a NIF-import diagnosability fix, not a material-translation boundary change
- [ ] **TESTS**: A regression test pins the new log lines firing on each of the three exit paths

---
