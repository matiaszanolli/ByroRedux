# #4411 — NIFAL-D9-2026-09-14-02: `every_source_derived_material_field_is_pinned_by_a_test` counts comment prose as a pin — its own rationale comment self-pins `alpha` and `alpha_threshold` (shared text-scan weakness in the Lights/Collision resolve scans)

**Labels**: low,nifal,test-gap,bug
**Filed from**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` via `/audit-publish` (2026-09-14)

**Source**: `docs/audits/AUDIT_NIFAL_2026-09-14.md` (`/audit-nifal`, HEAD `7374634f5`)

- **Severity**: LOW (test guard; no live masked regression today)
- **Dimension**: Completeness
- **Tier Violated**: harness-coverage gap
- **Game Affected**: all
- **Location**: `byroredux/src/material_translate.rs:2773-2783` (the `pinned` closure; its own comment at `:2776-2777`). Latent siblings: `crates/nif/src/import/walk/lights.rs:276-288` and `crates/nif/src/import/collision/mod.rs:669-685`, whose resolve-side scans cover whole source files, test modules and comments included.
- **Status**: NEW
- **Description**: The guard treats a field as pinned when any `;`-delimited chunk of the test half contains `material.<field>` at a word boundary plus the substring `"assert"` or `".expect("`. Chunks are not comment-stripped, and `"assert"` also matches English prose. The guard's own rationale comment — "Word boundary, so `material.alpha` is not satisfied by an assertion on `material.alpha_threshold`" — contains both needles, so those two fields are pinned by prose alone. The Lights and Collision resolve-side scans match `downcast_ref::<X>` anywhere in the file, so a future comment or test line naming an arm would mark it resolved even if the production arm were deleted.
- **Evidence**: The agent ran a verbatim port of the scanner on a scratch copy with the only two real assertions on `material.alpha` / `material.alpha_threshold` deleted; it still reported both as pinned. With `//` comments stripped, each field drops from 2 matching chunks to 1. The exterior-spawner guard, replayed comment-stripped, still holds on code text for all 6 files. The Lights and Collision scans have no false match today.
- **Impact**: The #3462 contract ("the next added copy cannot slip through") is weaker than stated. Together with NIFAL-D5-2026-09-14-02 and NIFAL-D7-2026-09-14-04, every text-scan or kitchen-sink completeness guard added since #3462 has at least one hole of this shape.
- **Related**: #3462, #4302, NIFAL-D5-2026-09-14-02, NIFAL-D7-2026-09-14-04. These three can reasonably be published as one guard-hardening issue.
- **Suggested Fix**: Strip `//`/`///` comments and string literals before matching, or require the needle inside an `assert…!(` / `.expect(` expression on the same statement. Reword the rationale comment so it cannot self-match. Scan only the production prefix (`split_once("#[cfg(test)]").0`) in `resolved_light_structs` / `resolved_shape_structs`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other load paths)
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `byroredux/src/material_translate.rs` (`translate_material`), `Material::resolve_pbr` (`crates/core/src/ecs/components/material.rs`), or the emitter params in `crates/nif/src/import/walk/emitter.rs` (`extract_emitter_params` / `extract_emitter_rate`), per-game logic stays at the NIFAL parser→canonical boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
