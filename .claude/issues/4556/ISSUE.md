# NIFAL-D3-2026-09-21-02: Stale light-doc contracts — "zero for ambient/point" direction (twice) and a phantom numeric kind tag

**Labels**: low, nifal, documentation, doc-rot

**Severity**: LOW · **Dimension**: Skinning/Lights · **Tier Violated**: — (docs misstate the canonical contract) · **Game Affected**: all
**Location**: `crates/nif/src/import/types.rs:34-36` and `:42-44`; `crates/core/src/lighting.rs:252-254`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
Since #4395, `imported_light_from_base` computes world-rotation column 0 for **every** kind and `Emitter::from_legacy_world_units` sanitizes it, so a NIF point/ambient light's canonical `direction` is a normalized non-zero unit vector. The documented "zero for ambient/point" contract is false in two places (renderer point branch ignores it, so behaviour is correct). The `kind` doc's "0 = ambient, 1 = directional…" numeric tag matches neither the `EmitterKind` enum order nor the GPU packing (Ambient|Point→0, Spot→1, Directional→2) — pre-enum prose.

### Evidence
`walk/lights.rs:138-151` (column 0 for all kinds, negated only for Directional); `render/lights.rs:77-81` (GPU packing).

### Impact
An auditor checking the "point direction is zero" contract finds it violated and may "fix" the boundary, or write a consumer relying on zero. The phantom kind tag invites a wrong downstream mapping.

### Related
#4395, #2205

### Suggested Fix
Update the three doc sites: direction is "column 0 of the world rotation for all kinds; consumers ignore it for ambient/point"; `kind` is the `LightKind`/`EmitterKind` enum, GPU packing lives in `gpu_light_from_emitter`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
