# NIFAL-D3-2026-09-21-01: ImportedSkin.global_skin_transform doc implies the runtime palette composes the transform — the code (and its own regression test) deliberately does not

**Labels**: low, nifal, documentation, doc-rot

**Severity**: LOW · **Dimension**: Skinning/Lights · **Tier Violated**: — (doc contract contradicts the canonical consumer; live regression trap) · **Game Affected**: all (trap fires on legacy body NIFs)
**Location**: `crates/nif/src/import/types.rs:1288-1299` vs `crates/core/src/ecs/components/skinned_mesh.rs:72-90`/`:167-182`, `byroredux/src/scene/nif_loader.rs:1795-1807`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
The raw-tier field doc's narrative ("OpenMW composes it into the runtime palette as the OUTERMOST factor … which is why our pre-Phase-1b.x palette produced the ribbon artifact") implies the current palette composes it. The post-#771 semantics — pinned by `palette_matches_nifly_skin_to_bone_semantics_with_non_identity_global` — is the opposite: `bind_inverses[i]` is nifly's compose-ready `transformSkinToBone` and `compute_palette_into` does **not** multiply `global_skin_transform` (doing so double-applies the offset).

### Evidence
`compute_palette_into` builds `palette[i] = bone_world × bind_inverses[i]` with no reference to the field; the field is documented "Informational / diagnostic only".

### Impact
A future change that "restores the documented invariant" by composing the transform would double-apply the offset on every legacy body NIF (Doc Mitchell class) and regress skinning while the doc points at the wrong authority.

### Related
#4410 (spec-side sibling), #771, M41.0 Phase 1b.x

### Suggested Fix
Rewrite the doc to state the resolution: captured (Y-up, identity for FO4+/BSSkin), informational/diagnostic only, NOT composed at runtime because per-bone `bind_inverses` already encode the skin-to-skeleton offset; `compute_palette_into`'s doc + its regression test are the authority.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
