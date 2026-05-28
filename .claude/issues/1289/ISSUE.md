Surfaced by the 2026-05-28 Starfield audit (`docs/audits/AUDIT_STARFIELD_2026-05-28.md` Dim 3). Follow-up to closed [#762 / SF-D6-03](https://github.com/matiaszanolli/ByroRedux/issues/762) — the parser landed; the consumer didn't.

## Issue

[#762](https://github.com/matiaszanolli/ByroRedux/issues/762) closed 2026-05-24 with the Starfield CDB parser landed in [`crates/sfmaterial/`](crates/sfmaterial/) (1048 LOC, binary `materialsbeta.cdb` reader following the `gibbed/Gibbed.Starfield` format reference). The lib.rs docblock at lines 44-49 explicitly defers consumer wiring:

> # Scope (Stage B per audit #762)
>
> This crate parses the binary CDB into a generic `Value` tree. The
> consumer-side mapping (Starfield-specific material → `ImportedMesh`
> fields) happens in `byroredux/src/asset_provider.rs` and is a
> separate concern from the format parsing here.

Today `grep -n 'sfmaterial\|ComponentDatabaseFile\|materialsbeta\.cdb' byroredux/src/asset_provider.rs` returns **zero hits**. The consumer-side mapping is unimplemented.

## Chain that breaks

1. Starfield mesh parses, stopcond at `crates/nif/src/blocks/shader.rs:674-752` captures `material_path = "materials/cargobay.mat"` ✓
2. Walker plumbs `material_path` into `MaterialInfo` ([`crates/nif/src/import/material/walker.rs:138`](crates/nif/src/import/material/walker.rs#L138)) ✓
3. [`pack_bgsm_material_flags`](byroredux/src/cell_loader.rs#L192-L194) checks `mesh.is_pbr` — **always false for Starfield** because nothing in the pipeline reads the CDB ✗
4. `flags = 0` → no `MAT_FLAG_PBR_BSDF` → `triangle.frag` skips the Disney BSDF branch → Starfield content renders through the legacy Lambert + simple-GGX path

## Risk

This is exactly the regression pattern the audit checklist warned about: *"the regression pattern is the opposite of FNV: Starfield materials silently rendering with Lambert because the PBR flag was never plumbed"*.

Starfield is **PBR-canonical authoring** — vanilla content expects metalness/roughness Disney BSDF, not Lambert. Visible symptom today: any Starfield material gets a magenta-checker albedo (missing texture) AND the lighting model is wrong on top of that. Once the .mat plumbing closes the texture side, the Lambert-vs-Disney mismatch becomes the dominant visual regression.

## Suggested fix (cheap → correct)

1. **Lift `ComponentDatabaseFile::parse(materialsbeta.cdb)` once at engine init** in `byroredux/src/asset_provider.rs` (analogous to BSA registry init). The CDB lives inside `Starfield - Materials.ba2`.
2. **Build a `Starfield material_path → MaterialFields` lookup table** keyed on the `.mat` path captured by the NIF stopcond. The CDB's flat instance stream (per `crates/sfmaterial/src/reader.rs`) needs walking once to populate the map.
3. **Extend `pack_bgsm_material_flags`** (or sibling `pack_sfmaterial_flags`) at [`byroredux/src/cell_loader.rs:177-213`](byroredux/src/cell_loader.rs#L177-L213) to consult the SF-material table when `mesh.material_path` has a `.mat` suffix. Set `BGSM_PBR | BGSM_AUTHORED` plus any translucency / model-space-normals bits the CDB authoring carries.

## Effort estimate

1-2 PRs. The hard binary-format work landed in #762; this issue is wiring + a single CDB walk to build the lookup table. The shader path already exists (Disney BSDF gates on `MAT_FLAG_PBR_BSDF` per [#1248-1252](https://github.com/matiaszanolli/ByroRedux/issues/1248)) — closing this finding makes that path reachable for Starfield content.

## Completeness Checks

- [ ] **UNSAFE**: N/A — no unsafe involved
- [ ] **SIBLING**: verify the same wiring exists for FO4 BGSM (`merge_bgsm_into_mesh`) and document the analogous Starfield CDB path; confirm `pack_bgsm_material_flags` name vs `pack_sfmaterial_flags` split decision (likely a rename to `pack_material_flags` if both paths converge on the same packer)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: if the CDB lookup table lives behind a `RwLock`, verify TypeId ordering with other asset-provider resources
- [ ] **FFI**: N/A
- [ ] **TESTS**: add a Starfield-specific test (under `BYROREDUX_STARFIELD_DATA=...`) that loads a known CDB-resolved mesh and asserts `MAT_FLAG_PBR_BSDF` ends up set on the `GpuMaterial`. Mirrors the FO4 BGSM regression coverage in `byroredux/src/cell_loader.rs::pack_bgsm_material_flags_tests`.
