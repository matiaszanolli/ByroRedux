# NIFAL-D1-2026-09-29-01: the spec names a #[cfg(test)]-only glass classifier as the live boundary step, and #4855's from_bgsm provenance input is recorded nowhere

**Labels**: low,documentation,doc-rot,nifal

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-29.md`
**Severity**: LOW
**Dimension**: Material · **Tier Violated**: doc / record-keeping · **Game Affected**: all
**Location**:
- `docs/engine/nifal.md` §3 step 4 ("classifies glass once, alpha-aware (`helpers::classify_glass_into_material`)") and the Layering note below it; the same §3 paragraph naming `cell_loader/placement_lod.rs` as the exempt production caller;
- `byroredux/src/material_translate.rs`, intra-doc link `[`crate::helpers::classify_glass_into_material`]` in `translate_material`'s rustdoc;
- `docs/engine/asset-pipeline.md` and `docs/engine/material-abstraction.md` (`helpers::classify_glass_into_material`);
- comments in `byroredux/src/asset_provider/material/merge.rs` (two sites).

## Description
`2b1b7fc5c` made `helpers::classify_glass_into_material` a `#[cfg(test)]` shim that passes `from_bgsm = false`. Production now calls `classify_glass_into_material_with_provenance` (9 arguments, including the new `from_bgsm`). The live boundary therefore consumes two distinct provenance signals:
- `external_material_resolved` gates keyword promotion of effect carriers (#4283);
- `from_bgsm` gates overriding an authored lit dispatch `2..=20` (#4855).

Every spec/doc site still names the shim, and none records the two-signal split. The same §3 paragraph lists `placement_lod.rs` as the Phase-2-exempt production caller but omits `object_lod.rs`, which has been a `translate_material` caller since #4245 and also attaches no `MaterialTextureHandles`.

## Evidence
`rg 'classify_glass_into_material\b' docs/engine byroredux/src` finds the non-test sites above; the definition in `byroredux/src/helpers.rs` is `#[cfg(test)] fn classify_glass_into_material`.

## Impact
An auditor or contributor following the spec reads a function production never calls; the rustdoc link resolves only under `cfg(test)`.

## Related
#4873 (open — the `terrain_lod_btr.rs` Phase-2 omission and ground-cover wording in the same §3 section; could be folded together); #4246, #4855, #4283.

## Suggested Fix
- Rename the references at those sites to `classify_glass_into_material_with_provenance`.
- Add the provenance pair (`external_material_resolved`, `from_bgsm`) to §3 step 4.
- Add `object_lod.rs` to the Phase-2 caller and exemption list.

Validated at HEAD 9fcfdc3fc: `helpers.rs` has `#[cfg(test)]` on `classify_glass_into_material` and `pub(crate) fn classify_glass_into_material_with_provenance`; nifal.md, asset-pipeline.md, material-abstraction.md, `material_translate.rs` rustdoc and merge.rs comments still name the shim.

## Completeness Checks
- [ ] **SIBLING**: `byroredux/src/render/static_meshes.rs` comments naming the shim updated too
- [ ] **CANONICAL-BOUNDARY**: doc changes only; `translate_material` behaviour unchanged
