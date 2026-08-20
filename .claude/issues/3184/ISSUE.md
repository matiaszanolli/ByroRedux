# NIFAL-D1-2026-08-20-02: WaterKind -> foam_strength is literals at three ESM sites and zero NIFAL sites — the same canonical kind renders with 3.25x the foam depending on which boundary produced it

Issue: https://github.com/matiaszanolli/ByroRedux/issues/3184
Finding: NIFAL-D1-2026-08-20-02
Labels: medium,nif-parser,renderer,bug
Source: docs/audits/AUDIT_NIFAL_2026-08-20.md

Filed from `docs/audits/AUDIT_NIFAL_2026-08-20.md` (Dimension 1 — Material, mesh-water slice). NIFAL canonical-translation finding — see `/audit-nifal`.

**Severity**: MEDIUM
**Tier violated**: `single-boundary` (a kind-derived canonical value with no single derivation site), manifesting as a divergent canonical output.
**Game Affected**: all games shipping mesh-bound river/stream/rapids water (Oblivion, FO3, FNV, Skyrim+).

**Location**: derivation **absent** at `byroredux/src/material_translate.rs:90` (`water_material_from_mesh`); the ESM-path literals are at `byroredux/src/env_translate.rs:932`, `:947` and `byroredux/src/cell_loader/water.rs:379`, `:381`.

## Read this together with its two siblings

One of **three** mesh-water findings from this sweep. Mesh water crossed NIFAL for the first time in this delta with its derivations centralised but *not* its boundary discipline — this is the duplication `translate_material` exists to prevent, re-created for a new category. The siblings are the 39-line verbatim spawn-block copy-paste and the four uncited constants (hardcoded world **+X** flow direction). Same fix pass.

## Description

`WaterKind` is the canonical enum, and the canonical type **already demonstrates the right pattern** for kind-derived values: `WaterFlow::speed_for_kind` (`crates/core/src/ecs/components/water.rs:453`) lives on the canonical type with its measurement rationale attached, and both boundaries call it.

`foam_strength` — the sibling kind-derived value — has no such function. Instead the mapping `Rapids -> 0.85` / `River -> 0.20` is typed out as literals at **three ESM-path sites**, and the NIFAL mesh-water path supplies **none** of them: `water_material_from_mesh` starts from `WaterMaterial::default()` (`foam_strength: 0.65`, `crates/core/src/ecs/components/water.rs:341`) and never touches the field — even though `water_kind_from_mesh_geometry` has already produced the kind two statements later at the call site.

## Evidence

```rust
// ESM/EXAL path — byroredux/src/env_translate.rs:929-948
if lowered.contains("rapid") || (…) { kind = WaterKind::Rapids; mat.foam_strength = 0.85; }
else if lowered.contains("waterfall") || … { kind = WaterKind::River; mat.foam_strength = 0.20; }

// ESM/EXAL path, second copy — byroredux/src/cell_loader/water.rs:378-382
if matches!(kind, WaterKind::Rapids) { material.foam_strength = 0.85; }
else if matches!(kind, WaterKind::River) { material.foam_strength = 0.20; }

// NIFAL path — byroredux/src/material_translate.rs:90-150
let mut water = WaterMaterial::default();   // foam_strength stays 0.65
water.shader_flags = material.water_shader_flags;
…                                            // no foam_strength assignment anywhere
```

Re-verified at HEAD: `grep -rn foam_strength byroredux/src crates/core/src` returns writes at exactly `env_translate.rs:932`, `env_translate.rs:947`, `cell_loader/water.rs:379`, `cell_loader/water.rs:381` — and nothing in `material_translate.rs`.

The value is live at the GPU: `byroredux/src/render/water.rs:227` uploads `mat.foam_strength` into `GpuWaterParams.timing.z`, which `crates/renderer/shaders/water.frag:602` reads as `foamStrength` and `:997` multiplies into the final foam mask.

## Impact

A river authored as a NIF **mesh** renders with **0.65** foam while an identical river authored as a WATR-backed **cell plane** renders with **0.20** — **3.25x too much foam** on exactly the seam where the two meet. A mesh river segment flowing into a cell water body is the common authoring pattern in Oblivion and Skyrim exteriors. Rapids diverge the other way (0.65 vs 0.85).

Because the derivation exists only as literals, no test or type can observe the divergence.

## Suggested Fix

Add `WaterMaterial::foam_for_kind(kind) -> f32` (or `WaterKind::canonical_foam_strength`) to `crates/core/src/ecs/components/water.rs` **next to `speed_for_kind`**, carrying the rationale comment the literals currently carry nowhere; call it from all three ESM sites and from the mesh-water path. That collapses four hand-written numbers into one and makes the NIFAL/EXAL agreement structural rather than coincidental.

## Related

- Sibling findings from the same sweep (the duplicated spawn block; the uncited constants).
- #2872 — the audit that established `WaterFlow::speed_for_kind` as the canonical kind-derived-value pattern this one should follow.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: the mapping lands on the canonical type in `crates/core`, not in a per-path helper; per-game/per-path logic stays at the boundary. See `/audit-nifal`.
- [ ] **SIBLING**: all four literal sites replaced (`env_translate.rs` x2, `cell_loader/water.rs` x2) **and** the NIFAL path wired up — a partial fix leaves the divergence in place
- [ ] **RATIONALE**: the 0.85 / 0.20 / 0.65 values carry their source at the new single site (they currently carry it nowhere)
- [ ] **TESTS**: a test asserts NIFAL and EXAL produce the same `foam_strength` for the same `WaterKind`
