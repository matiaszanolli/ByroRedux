# #1353 — D8-07: BgsmFile.greyscale_texture not forwarded — FO4 NPC/creature color variants missing

_Snapshot from AUDIT_FO4_2026-05-30. GitHub is authoritative for live state._

**Severity**: MEDIUM · **Source**: AUDIT_FO4_2026-05-30 (D8-07) · **Domain**: nif-parser / legacy-compat

**Location**: `crates/bgsm/src/bgsm.rs:28,186` (parsed); `byroredux/src/asset_provider.rs::merge_bgsm_into_mesh` (not forwarded); `crates/nif/src/import/types.rs` (`ImportedMesh` — no field for it)

**Description**: `BgsmFile.greyscale_texture` (palette-LUT texture path, present in BGSM v≤2) is correctly parsed from the BGSM file at `bgsm.rs:186`, but `merge_bgsm_into_mesh` has no destination field on `ImportedMesh` for it — it is silently dropped. Similarly `BgemFile.grayscale_texture` (parsed in `bgem.rs`) is not forwarded.

The greyscale LUT is the mechanism by which FO4 authors NPC/creature color variants (skin tones, fur colors, faction liveries) — the same texture-palette concept as the `BSEffectShaderProperty.greyscale_texture` the engine already supports via `GreyscaleLutHandle` (#890/#1341). Without it, all FO4 BGSM-authored character/creature variants render with the base texture color only.

**Evidence**: `bgsm.rs:28`: `pub greyscale_texture: String;` — parsed but no corresponding forwarding in `merge_bgsm_into_mesh`.

**Suggested Fix**: 
1. Add `pub greyscale_lut_path: Option<String>` to `ImportedMesh`
2. In `merge_bgsm_into_mesh`: `mesh.greyscale_lut_path = Some(leaf.greyscale_texture.clone())` (when non-empty)
3. In `translate_material` / draw pipeline: resolve via `resolve_texture`, attach as `GreyscaleLutHandle` (the same component pattern used for BSEffect greyscale — see #890)
4. Pair with `drop_texture` on cell unload (the same pattern as #1341 for `GreyscaleLutHandle`)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Resolution must happen at the NIFAL translate boundary, not per-draw
- [ ] **DROP**: Ensure the resolved handle is included in the unload victim walk (same fix class as #1341)
- [ ] **SIBLING**: `BgemFile.grayscale_texture` has the same gap — fix both in the same pass
- [ ] **TESTS**: Add a test asserting that a BGSM with a non-empty `greyscale_texture` produces an `ImportedMesh` with a populated `greyscale_lut_path`
