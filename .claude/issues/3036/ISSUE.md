# FNV-2026-08-16-D1-01: BSXFlags bit 5 drops the whole NIF, deleting 223 real FNV meshes

**Issue**: #3036
**Severity**: HIGH
**Dimension**: 1 — Cell Loading
**Labels**: `high,nif-parser,legacy-compat,bug`
**Source report**: `docs/audits/AUDIT_FNV_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_FNV_2026-08-16.md` (Dimension 1 — Cell Loading End-to-End).

**Location**: `byroredux/src/cell_loader/references/import.rs`:84-98 · `byroredux/src/cell_loader/partial.rs`:55-66

## Description

Both NIF-import entry points (the synchronous REFR path and the exterior-streaming drain) treat `BSXFlags & 0x20` on pre-FO4 content as *"this whole NIF is an editor marker"* and return `None`, discarding meshes, collision, lights and emitters alike.

nif.xml is explicit that the bit means the file **contains** editor-marker children:

> `/mnt/data/src/reference/nifxml/nif.xml:4305` — `Bit 5 : EditorMarkers present, bEditorMarker(Skyrim)`

The correct behaviour is to **cull the marker child node, not the model**.

The over-broad drop was introduced by `docs/audits/AUDIT_POSITIONING_DECALS_2026-04-13.md` PD-01, which stated "BSXFlags bit 5 marks an **entire NIF** as 'editor marker — do not render'". The 2026-07 `bsver < FALLOUT4` carve-out narrowed the damage to exactly the pre-FO4 games — Oblivion, FO3 and FNV.

## Evidence

```rust
// byroredux/src/cell_loader/references/import.rs:89-98 (re-verified 2026-08-17)
let bsx_editor_marker = bsx & 0x20 != 0 && bsver < byroredux_nif::version::bsver::FALLOUT4;
if bsx_editor_marker {
    log::debug!("Skipping editor marker NIF '{}' (BSXFlags 0x{:X}, BSVER {})", label, bsx, bsver);
    return None;
}
```

**Live, Prospector Saloon** (`mesh.cache failed` over `byro-dbg`): `meshes\furniture\stool01.nif`, `meshes\furniture\barkeep.nif`, `meshes\effects\ambient\fxbasicglowbillboard.nif`.

`stool01.nif` is a genuine stool, not a marker. `trace_block` shows a `BSFadeNode` root, a `BSFurnitureMarker`, a `bhkRigidBody` + `bhkMoppBvTree` collision chain, and **two** `NiTriStrips` — one small one with `BSShaderNoLightingProperty` + `NiAlphaProperty` (the marker), one with `BSShaderPPLightingProperty` and a `BSShaderTextureSet` pointing at `textures\furniture\Stool01.dds` + `Stool01_n.dds` backed by a 30,190-byte `NiTriStripsData`. Its `BSXFlags` = `0x22` = bit 1 (collision) | bit 5 (markers present).

`barkeep.nif` (`0x20`, 1 node, 0 meshes) genuinely *is* a pure marker and is correctly skipped — the distinction the current code cannot make.

**Live, exterior** `--grid 0,0 --radius 3` WastelandNV — three more real assets in `mesh.cache failed`, each verified to carry geometry: `inddoorsmanim01.nif` (BSX `0x2B`, 3 meshes / 2 colliders), `barrel02firelight.nif` (`0x33`, 1 / 1), `supermutantbedding01.nif` (`0x22`, 2 / 1).

**Corpus sweep** over all 14,881 NIFs in `Fallout - Meshes.bsa`:
```
pre-FO4 with BSX bit5 set:                              336
  ... pure markers (0 meshes, 0 colliders):             113
  ... carrying real geometry/collision (DROPPED TODAY): 223
```

## Impact

**223 real FNV meshes are deleted at import** — furniture, animated doors, fire-lit barrels, bedding — along with their collision, lights and emitters. The same rule applies to Oblivion and FO3.

The failure is silent past a `log::debug!`, and it removes collision as well as geometry, so affected props are both invisible and non-solid.

## Suggested Fix

Cull the editor-marker **child node** during the walk rather than rejecting the file. The marker child is identifiable by its own properties (`BSShaderNoLightingProperty` + `NiAlphaProperty` on a small `NiTriStrips`, per the `stool01.nif` trace) — and a NIF whose only content is the marker then naturally imports to zero meshes, which the existing empty-import path already handles.

That reproduces the correct behaviour for `barkeep.nif` without discarding `stool01.nif`.

## Related

- `AUDIT_POSITIONING_DECALS_2026-04-13.md` PD-01 (the incorrect premise this rule came from)
- The 2026-07 `bsver < FALLOUT4` carve-out (narrowed the blast radius but did not fix the rule)

## Completeness Checks
- [ ] **BOTH-ENTRY-POINTS**: `references/import.rs` and `partial.rs` both fixed — the rule is duplicated
- [ ] **SIBLING**: Oblivion and FO3 verified too; the rule is not FNV-specific
- [ ] **MARKER-STILL-CULLED**: `barkeep.nif` still yields nothing; `stool01.nif` yields its geometry and collision
- [ ] **PREMISE**: The PD-01 claim is corrected in the audit record so it cannot be re-applied
- [ ] **TESTS**: A regression test imports a bit-5 NIF carrying geometry and asserts the mesh survives

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3036 --json state` when live state is needed.*
