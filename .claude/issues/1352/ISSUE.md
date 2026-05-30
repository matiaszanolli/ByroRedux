# #1352 — D7-03: Vanilla FO4 BGSM content uses Lambert diffuse instead of Disney BSDF

_Snapshot from AUDIT_FO4_2026-05-30. GitHub is authoritative for live state._

**Severity**: MEDIUM · **Source**: AUDIT_FO4_2026-05-30 (D7-03) · **Domain**: renderer / legacy-compat / NIFAL

**Location**: `byroredux/src/asset_provider.rs:989-1005` (Starfield `.mat` arm sets `is_pbr`); `crates/renderer/shaders/triangle.frag:2610, 2861` (Disney diffuse gate)

**Description**: `MAT_FLAG_PBR_BSDF` (which activates the Disney diffuse lobe at `triangle.frag:2610/2861`) is only set when `mesh.is_pbr == true`. `mesh.is_pbr` is set exclusively for Starfield `.mat` paths (when `has_starfield_cdb()` is true) and for BGSM files where `bgsm.pbr == true`. Bethesda virtually never authors `pbr=true` in vanilla FO4 BGSM files (sampled: 0 of 793 BGSMs in `Fallout4 - Materials.ba2`).

FO4 BGSM content instead sets `mesh.from_bgsm = true`, which activates `BGSM_AUTHORED` and the spec-glossiness F0 derivation path (`triangle.frag`), but NOT the Disney diffuse lobe — it takes the Lambert diffuse path. This is not a data-corruption bug (the GGX specular is correct; the Lambert diffuse is what Bethesda's original pipeline used). It is a rendering quality gap.

**Suggested Fix**: After RenderDoc visual validation comparing Lambert vs Disney diffuse on FO4 BGSM content:
```rust
// In merge_bgsm_into_mesh, after setting mesh.from_bgsm = true:
mesh.is_pbr = true; // route through Disney diffuse, not Lambert
```
Gate on `mesh.from_bgsm` (not `bgsm.pbr`) so all FO4 BGSM-authored content gets the Disney path regardless of the per-BGSM PBR flag. This requires RenderDoc sign-off to verify no content looks worse with the Disney diffuse lobe than with Lambert.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The `is_pbr` flag flows from `merge_bgsm_into_mesh` through `pack_bgsm_material_flags` to `triangle.frag` — confirm it stays at the NIFAL parser→Material boundary and is not re-derived at render time
- [ ] **SIBLING**: Verify FO3/FNV content (no BGSM, `from_bgsm=false`) is unaffected by this change
- [ ] **TESTS**: Add a visual regression test (e.g. a known FO4 armor mesh compared before/after Disney diffuse activation)
