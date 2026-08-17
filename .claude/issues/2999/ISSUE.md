# FO4-D5-08: FaceTint slot-4/5 None arms rest on Skyrim occupancy false on FO4

**Issue**: #2999
**Severity**: MEDIUM
**Dimension**: 5 — shader flags / slot routing
**Labels**: `medium,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FO4_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FO4_2026-08-16.md` (Dimension 5 — shader flags / slot routing).

**Location**: `crates/nif/src/import/material/slot_role.rs`:126-133

## Description

Slots 4 and 5 return `None` for shader types 4/5/6, justified in-code (#1350) as *"FaceTint's slots 4/5 are absent on 100% of vanilla properties"* and defended as guarding against a mis-exported NIF binding a spurious env cube.

**That occupancy claim is a Skyrim measurement.** On FO4 both slots are routinely authored on FaceTint heads and carry real, distinct data.

## Evidence

`Fallout4 - Meshes.ba2`, shader type 4, n = 1,229:

- **slot 4: 623 non-empty (50.7%)** — all `Shared/Cubemaps/mipblur_DefaultOutside1_dielectric.dds`, a genuine environment cubemap
- **slot 5: 981 non-empty (79.8%)** — all `_n`, all wrinkle/crease normals:
  - `Actors/Character/BaseHumanFemale/BaseFemaleHeadWrinkles_n.DDS`
  - `Actors/Character/BaseHumanMale/HeadWrinkles_n.dds`
  - `Actors/Synths/Gen2SkinHeadCrease_n.dds`
  - `Actors/Supermutant/SupermutantHeadCrease_n.dds`
- `MeshesExtra` reproduces both (type 4: 15 and 66 respectively)
- Type 1 slot 5 additionally holds 40 `_m` entries

As with slot 3, **the canonical destination already exists and is unreachable**: `MaterialTextureSet::wrinkle` (`crates/nif/src/import/types.rs`:327, bindless index 14), which the TXST decode also produces (`crates/plugin/src/esm/cell/support.rs`:447-455).

Re-verified 2026-08-17: the `SKIN_TINT | HAIR_TINT | FACE_TINT => None` arms are present on both slot 4 and slot 5, with the in-code comment still asserting the Skyrim occupancy claim.

## Impact

Every FO4 humanoid, synth and super-mutant head silently loses its environment cubemap and its wrinkle/crease normal map at import. Faces render flatter and without the expression-driven crease detail the FO4 head system authors.

Because the arm returns `None` **the loss is invisible** — no warning, no diagnostic.

## Suggested Fix

Add a `TextureRole::Wrinkle` variant, and gate the slots-4/5 `None` on the **Skyrim-family path** rather than applying it to every game.

## Related

- #1350 (the arm being generalised)
- #2997 (FO4-D5-06 — same "Skyrim evidence generalised to FO4" root cause and same seam)

## Completeness Checks
- [ ] **SIBLING**: All three FO4 slot findings (#2997, #2998, this) fixed at one game-awareness seam
- [ ] **CANONICAL-BOUNDARY**: The per-game gate lives at the parser→`Material` boundary
- [ ] **NO-DEAD-ROLE**: `wrinkle` reachable from the NIF path, not only from the TXST decode
- [ ] **IN-CODE-CLAIM**: The `#1350` comment's occupancy claim re-scoped to Skyrim, so it cannot be re-generalised
- [ ] **TESTS**: A regression test asserts FO4 FaceTint slots 4/5 land in the cubemap and wrinkle roles

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 2999 --json state` when live state is needed.*
