**Severity**: LOW (observability; block_size recovery masks the symptom)
**Dimension**: Property → Material Mapping (parser-side)
**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md` D4-NEW-01

Parsing a vanilla FO4 interior precombined `_oc.nif` file (`Fallout4 - MeshesExtra.ba2 → meshes\precombined\00001e5d_03ebca62_oc.nif`, Dugout Inn cell) produces a consistent stream-drift WARN:

```
NIF parse: 23 block(s) parsed Ok but consumed != block_size;
stream realigned by header size table: BSLightingShaderProperty=23
```

Every BSLightingShaderProperty block in the file under-reads. The block_size table recovers the stream so downstream parsing succeeds, but the four bytes per BSLSP are silently absorbed into the recovery skip.

### Evidence
- `nif_stats /tmp/dugout_pc0.nif` reports `BSLightingShaderProperty=23` drift events; ALL 23 BSLSP blocks under-read by 4 bytes.
- Regular FO4 BSVER-130 NIFs (non-precombined) parse with zero drift on BSLSP per the 2026-04-26 100% parse-rate sweep.
- Discriminator: BSPackedCombinedSharedGeomDataExtra extra_data ref on the parent NiNode.

### Impact
Today: cosmetic — the geometry path drops on `num_vertices == 0`. When CSG support lands, four bytes of the trailing BSLSP layout are silently zeroed per shader instance.

Likely candidate field: `Root Material` NiFixedString sidecar in shared-precombined content (where the material is referenced from absorbed REFRs' TXST overrides, not from a per-BSLSP path).

### Suggested Fix
Diagnose empirically: add a hex dump probe to capture the four bytes consumed by the recovery skip, compare to a regular BSVER-130 NIF, identify the field, and gate the read appropriately.

### Completeness Checks
- [ ] **SIBLING**: Same drift on BSEffectShaderProperty in precombined NIFs (not yet observed; verify).
- [ ] **TESTS**: nif_stats regression on `Fallout4 - MeshesExtra.ba2` after the fix should show zero BSLSP drift events.
