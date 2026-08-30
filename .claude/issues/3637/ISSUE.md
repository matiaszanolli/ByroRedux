# #3637: FO4-2026-08-30-D1-01: mesh/texture archive lookup is CLI-order-first-wins, silently shadowing 1,871 DLC overrides and the entire 3,851-entry HD texture pack

**Source**: `docs/audits/AUDIT_FO4_2026-08-30.md` — Dimension 1
**Severity**: MEDIUM
**Location**: `byroredux/src/asset_provider/texture.rs` — `TextureProvider::extract_mesh`, `TextureProvider::extract`, `TextureProvider::has_texture`, `build_texture_provider`

## Description

Mesh and texture lookup iterates the archive chain and returns the **first** hit;
`build_texture_provider` appends archives in raw `--bsa` / `--textures-bsa` CLI order with no
load-order alignment, no last-wins rule, and no shadow diagnostic. Bethesda's own precedence
is the opposite: a later archive overrides an earlier one.

## Evidence

Current code (verified 2026-08-30) — both extractors are plain first-wins loops:

```rust
pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
    let normalised = normalize_mesh_path(path);
    for archive in &self.mesh_archives {
        if let Ok(data) = archive.extract(normalised.as_ref()) { return Some(data); }
    }
    None
}
```

`extract` is the same shape; `has_texture` uses `.any()`, which is order-insensitive and
therefore inconsistent with what `extract` will actually return.

MEASURED against the installed FO4 `Data/` (297 entries, 187 `.ba2`, 7 masters):

- **1,681 `_oc.nif` paths exist in BOTH `Fallout4 - MeshesExtra.ba2` and a DLC
  `- Main.ba2`.** All 1,681 differ byte-for-byte, **and all 1,681 name a different
  `BSPackedGeomObject::filename_hash`** (base → `ddf19a67` / `Fallout4 - Geometry.csg`;
  DLC → the DLC's own blob). Whichever archive is listed first decides which *bake* renders —
  silently, because the #2369 hash routing makes both decode cleanly.
- Total DLC mesh entries shadowed when the base archives are listed first: **1,871**
  (1,761 precombines + 110 ordinary meshes) — DLCCoast 633, DLCNukaWorld 559, DLCRobot 427,
  DLCworkshop03 249, DLCworkshop01 3.
- Textures: 4 DLC entries, plus **`DLCUltraHighResolution`: 3,851 of 3,851 entries (100%)
  collide with `Fallout4 - Textures*.ba2`** — the HD pack is entirely inert unless the user
  happens to list it first.

## Impact

The natural invocation — archives in the same order as `--master`/`--esm`, which is how every
documented FO4 command line in the repo is written — is exactly the broken one. DLC
precombine re-bakes are silently replaced by their base-game namesakes, and the entire HD
texture pack never loads. Nothing logs.

## Suggested Fix

Reverse-iterate the chain (or insert at the front on open) so later archives win, and log a
shadow count per archive at open time so a collision is visible rather than silent. Bring
`has_texture` into agreement with whichever `extract` resolves.

## Related

#2369 (CSG hash routing — the reason both bakes decode cleanly and the shadow is invisible),
#1590 (owning-plugin path model).

## Completeness Checks
- [ ] **SIBLING**: `extract`, `extract_mesh`, `has_texture` and any other chain walker (material `.ba2`, script `.pex` lookup in `asset_provider/script.rs`) must all adopt the same precedence
- [ ] **TESTS**: a regression test pins a path present in two archives resolving to the later-listed one, plus the shadow-count diagnostic firing
