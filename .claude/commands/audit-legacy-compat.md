---
description: "Audit compatibility gaps between Gamebryo 2.3 and Redux — what's mapped, what's missing"
---

# Legacy Compatibility Audit

Read `_audit-common.md` and `_audit-severity.md` for shared protocol.

## Purpose

Compare the Gamebryo 2.3 architecture against Redux's current implementation.
Identify gaps that block NIF loading, animation playback, or content rendering.

## Dimensions

### 1. Scene Graph Decomposition
- Read `docs/legacy/api-deep-dive.md` mapping table
- For each NiAVObject field: does the Redux component exist?
- Missing components needed for NIF import (Parent, Children, WorldTransform, WorldBound, etc.)

### 2. NIF Format Readiness
- `crates/nif` is mature (the live dispatch arm count is in `crates/nif/src/blocks/mod.rs`); audit
  coverage *gaps*, not "does a parser exist" — the bootstrap question is answered.
- Version coverage: which NIF versions decode? Which Gamebryo object types are still NiUnknown-demoted?
- Link resolution: are cross-references (`BlockRef`) between objects resolved in the post-link phase?
- Minimum import set still intact (NiNode, NiTriShape, NiTriShapeData) — and the modern packed
  variants (BSTriShape, `BSGeometry`) that replaced them per game era.

### 3. Transform Compatibility
- Gamebryo: NiTransform = Matrix3 rotation + Point3 translation + float scale
- Redux: Transform = Quat rotation + Vec3 translation + f32 scale
- Is there a conversion function (Matrix3 → Quat) for NIF import?
- Is world transform propagation implemented (local * parent = world)?

### 4. Property → Material Mapping
- Gamebryo has 12 NiProperty types (alpha, texturing, material, zbuffer, etc.)
- Which map to Vulkan pipeline state vs per-object components?
- The `Material` component is mature/canonical (`crates/core/src/ecs/components/material.rs`); the live
  question is no longer "does it exist" but **"is `translate_material` the single boundary that produces
  it, and is PBR fully resolved?"** — `metalness`/`roughness` are plain `f32` (no `Option` + per-draw
  `classify_pbr`), filled from NaN sentinels by `Material::resolve_pbr` (clamps roughness to `[0.04, 1.0]`).
  This is the NIFAL material slice — **see also `/audit-nifal`** and Dimension 8 below.
- **NiFogProperty is parsed but intentionally not dispatched** (#1224 / D4-NEW-02
  closeout). Per-node fog override has no landing site on `Material` and the
  renderer's fog path reads cell-scope `CellLighting` only; observed corpus is
  1 vanilla FO3 block. Do not re-file as a finding — see `walker.rs` near the
  end of `extract_material_info` for the deliberate-skip comment.

### 5. Animation Readiness
- Gamebryo: NiTimeController → NiInterpolator → keyframes
- Redux: any keyframe data structures? Interpolation system?
- KF/KFM file format: parser exists?

### 6. String Interning Alignment
- Gamebryo: NiFixedString with GlobalStringTable
- Redux: StringPool + FixedString
- Are they semantically equivalent? Any gaps?

### 7. NIFAL Canonical-Translation Contract
The Gamebryo→Redux semantic mapping is now formalised as the NIF Abstraction Layer
(three tiers: raw `Imported*` → `translate()` → canonical ECS). This dimension audits the
*shape* of that mapping; the per-slice dimensions below audit the contents.
- Entry points: `docs/engine/nifal.md` (§1 three-tier model, §2 per-category leak inventory)
- Checklist:
  - Each per-category slice (material / geometry / skinning / lights / nodes / particles / collision)
    has **exactly ONE `translate()` boundary** — no second site re-derives the canonical form.
  - No `Option` "resolve-later" leaks downstream of the boundary (the canonical tier is the
    source of truth; the raw `Imported*` tier is allowed to be messy).
  - No per-game `if game == …` branches downstream of the boundary (per the format-translation memory).
  - Cross-check newly-closed leaks against `nifal.md` §2 so audits stop re-filing them.
  - **See also `/audit-nifal`** for the dimension-level checklist of this layer.

### 8. Material Translation Boundary
- Entry points: `byroredux/src/material_translate.rs::translate_material` (line 65),
  `crates/core/src/ecs/components/material.rs::resolve_pbr` (line 588), `nifal.md` §3 (reference realisation)
- Checklist:
  - `translate_material` is the **sole** `Material` producer — both spawn paths delegate to it
    (`byroredux/src/cell_loader/spawn.rs:861` and `byroredux/src/scene/nif_loader.rs:808`); no
    other site constructs a populated `Material`.
  - `metalness`/`roughness` are resolved `f32` (NaN sentinel filled by `resolve_pbr` via
    `classify_pbr_keyword`, then clamped) — the pre-canonical `Option`-override + render-time
    `classify_pbr` regression must NOT reappear (the deleted path is referenced only in comments,
    e.g. `byroredux/src/render/static_meshes.rs:270`).
  - Glass / cloth / metal classification happens once at the boundary, alpha-aware — never re-classified per draw.
  - Canonical material boundary landed in commit `3ce98db8`. **See also `/audit-nifal`.**

### 9. Particle Emitter Translation Parity
- Entry points: `crates/nif/src/blocks/particle.rs` (`NiPSysEmitter` / `NiPSysEmitterCtlr` /
  `NiPSysEmitterCtlrData` / `NiPSysGrowFadeModifier` typed blocks),
  `crates/nif/src/import/walk/mod.rs::extract_emitter_params` (line 670) / `extract_emitter_rate` (line 713),
  `byroredux/src/systems/particle.rs::apply_emitter_params` (line 29), `nifal.md` §2 (Particles slice)
- Checklist:
  - The four `NiPSys*` blocks decode as typed blocks (not the old opaque controller stack);
    `extract_emitter_*` lift the authored base kinematics + birth rate + GrowFade `base_scale`.
  - `apply_emitter_params` lets authored params **override** the name-heuristic presets (not the
    other way round) — the preset is the fallback when authoring is absent.
  - Flag any new `NiPSys*` block in the legacy 2.3 source not yet typed/translated.
  - Slice landed across commits `5708b5b9` / `9db60714` / `8f856d35`. **See also `/audit-nifal`.**

### 10. Havok Collision-Shape Coverage Matrix
- Entry points: `crates/nif/src/import/collision.rs::resolve_shape` (line 261),
  `crates/nif/src/blocks/collision/` (13 `bhk*Shape` variants), `nifal.md` §2 (Collision slice)
- Checklist:
  - Every parsed `bhk*Shape` resolves to a `CollisionShape` — all 13 dispatched variants have a
    matching `downcast_ref` arm in `resolve_shape`; no silent "unsupported shape" drop (the closed
    `BhkMultiSphereShape` → `Compound`/`Ball` and `BhkConvexListShape` → `Compound`-of-sub-shapes
    leaks were exactly that, commit `9c6096aa`).
  - Document the known limitations as **limitations, not findings**: `BhkNPCollisionObject`
    (FO4+ Havok-serialised blob, falls back to `cell_loader/spawn.rs::synthesize_static_trimesh`,
    commit `15016ee0`) and `BhkPCollisionObject` phantoms (need a `TriggerVolume` ECS path).
  - **See also `/audit-nifal`.**

### 11. Translation-Source Gaps (CDB + node passthrough)
Not every canonical input arrives via inline NIF shader properties; some sources sit *before* the
translate boundary, and some raw fields are deliberately parked with no consumer.
- Entry points: `crates/sfmaterial/` (Starfield `materialsbeta.cdb` reader), `byroredux/src/asset_provider.rs`
  (BGSM merge), `nifal.md` §2 (Nodes slice — passthrough triage), §4 (emissive ground-truth)
- Checklist:
  - **Starfield material source**: materials originate from the `sfmaterial` CDB (Component Database),
    NOT inline NIF shader props; they flow through `asset_provider` before `material_translate`. Verify
    the CDB→canonical path still funnels through the single `translate_material` boundary.
  - **Emissive scale**: the `EmissiveSource` discriminator (`Material` / `Lighting` / `Effect`,
    `crates/core/src/ecs/components/material.rs:354`) is a measured no-op (~1.0 shared scale across
    Oblivion/FNV/Skyrim/FO4 per `nifal.md` §4) — do not re-file an emissive-normalization finding.
  - **Node passthrough**: `bs_value_node` / `bs_ordered_node` / `tree_bones` / `range_kind` are
    raw-tier-parked with zero consumers (deferred deliberately, `nifal.md` §2 Nodes table) — record
    as known-bounded gaps, not findings.
  - **See also `/audit-nifal`.**

## Process

1. Read Redux component implementations in `crates/core/src/ecs/components/`
2. Cross-reference against Gamebryo headers in the legacy source
3. For each gap: classify as CRITICAL (blocks NIF loading), HIGH (blocks rendering), MEDIUM (blocks full fidelity), LOW (cosmetic)
4. Save report to `docs/audits/AUDIT_LEGACY_COMPAT_<TODAY>.md`
