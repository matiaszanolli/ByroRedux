# #1346 — D7-01: NIFAL PBR classifier runs at import, not translate() boundary

_Snapshot as filed from AUDIT_FNV_2026-05-30 (d7-01). GitHub is authoritative for live state — query `gh issue view 1346 --json state`._

**Severity**: MEDIUM · **Dimension**: NIFAL Canonical Translation · **Source**: AUDIT_FNV_2026-05-30 (D7-01) · cross-ref `/audit-nifal`

**Location**: `crates/nif/src/import/mesh/ni_tri_shape.rs:184,246` (physical classify site) ; doc claims at `byroredux/src/material_translate.rs:58-61` and `crates/core/src/ecs/components/material.rs` (`resolve_pbr` doc ~568-590)

**Description**: For FNV legacy inline-shader content the keyword PBR classifier (`classify_pbr_keyword`, via `mat.classify_legacy_pbr`) physically runs at NIF **import** time — ni_tri_shape.rs:184 + :246 writes `metalness_override: Some(...)`/`roughness_override: Some(...)` onto the `ImportedMesh`. At the NIFAL boundary, `translate_material` (material_translate.rs:149) seeds `mesh.metalness_override.unwrap_or(NaN)`; since the FNV override is always `Some`, `Material::resolve_pbr` sees a non-NaN value, **skips its classifier arm**, and only clamps. So the structure is "classify-at-import + clamp-at-translate", NOT "classify-at-translate" as the docs imply.

**Evidence**: ni_tri_shape.rs:184 `let legacy_pbr = mat.classify_legacy_pbr(pool);`, :246 `metalness_override: Some(legacy_pbr.metalness)`. material_translate.rs:149 `metalness: mesh.metalness_override.unwrap_or(f32::NAN)`. The doc at material.rs (`resolve_pbr`) says "resolved … once, at the translation boundary" — misleading for NIF-imported content. The classifier's own arm in `resolve_pbr` is dead for any source whose extractor populated the override (every NIF-imported mesh).

**Impact**: No wrong/divergent `Material` today — values are correct, just resolved one stage earlier than the docs imply. Risk is architectural: a future maintainer trusting "classification happens at the single NIFAL boundary" could remove the importer's classify call (silently dropping FNV PBR to the clamp-only path on a NaN seed) or add a second divergent classify. Single-source-of-truth is preserved only by *delegation* to the shared `classify_pbr_keyword` free fn.

**Suggested Fix**: Doc-only. Amend `resolve_pbr` + `material_translate.rs` docs to state that for NIF-imported content the classifier already ran at import (`import/mesh/*.rs` via `classify_legacy_pbr`) and `resolve_pbr` is a **clamp + sentinel-backstop**, not the classification site. Note the single source of truth is the shared `classify_pbr_keyword`. Optionally rename `resolve_pbr` → `clamp_and_backstop_pbr`.

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: Confirm the per-game classification stays at the parser→`Material` path (import extractors + the shared free fn) and is never pushed into the shader or re-derived at render time.
- [ ] **SIBLING**: Same doc-mismatch applies to `bs_tri_shape.rs:122,250` and `bs_geometry.rs:206,254` (Skyrim/Starfield import paths) — fix the framing consistently.
- [ ] **TESTS**: Optional — a test asserting that a NIF-imported FNV mesh reaches `resolve_pbr` with a `Some` override (clamp-only path), documenting the contract.
