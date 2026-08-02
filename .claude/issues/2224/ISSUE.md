# REN-D2-01: Fire-refraction proxies remain SHADOW_MASK_OPAQUE occluders despite a comment claiming TLAS exclusion

Severity: high
Source audit: docs/audits/AUDIT_RENDERER_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2224

**Dimension**: 2 (SSBO/ray queries), corroborated by Dimension 1 (AS masks) and Dimension 11 (G-buffer overwrite)
**Location**: `crates/renderer/shaders/triangle.frag` (fire-refraction branch, ~line 858); `crates/renderer/src/vulkan/acceleration/predicates.rs` (`shadow_mask_for_instance`, line 594)
**Status**: NEW

**Description**: The shader comment says the fire-refraction proxy "is excluded from BLAS/TLAS, so this ray
cannot hit the haze mesh itself and the proxy cannot cast shadows." The CPU
side does no such exclusion — `shadow_mask_for_instance` hands fire-refraction
proxies `SHADOW_MASK_OPAQUE` (the base mask before the `EFFECT_SHADER`/`FIRE_REFRACTION` exclusion check, which only gates the *additional* `SHADOW_MASK_STRUCTURE` bit, not the base `OPAQUE` bit), so they occlude shadow rays from every other
surface. A campfire's heat-haze plane produces a dark rectangle in the
shadow term around the very light it's meant to be transparent to.

**Evidence**: `predicates.rs:594-614` — `mask = SHADOW_MASK_OPAQUE` is assigned unconditionally before the `structural_material` check (which excludes `MATERIAL_KIND_FIRE_REFRACTION` only from the *additional* `SHADOW_MASK_STRUCTURE` bit). `triangle.frag` line ~859: "The proxy is excluded from BLAS/TLAS, so this ray cannot hit the haze mesh itself and the proxy cannot cast shadows or feed GI" — contradicted by the CPU-side mask assignment.

**Impact**: Heat-haze/fire-refraction proxies incorrectly self-shadow the fire they're meant to represent, producing a visible dark rectangle artifact around campfires and similar effects on Skyrim/FO4 content.

**Related**: REN-D11-02 (same material kind, G-buffer overwrite — shares root cause per the report's Prioritized Fix Order)

**Suggested Fix**: either add the material kind to the shadow-transport skip predicate (matching `MATERIAL_KIND_EFFECT_SHADER`'s treatment), or actually exclude the proxy from the TLAS as the comment claims — pick one and add a positive test.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
