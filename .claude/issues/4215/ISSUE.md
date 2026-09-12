# TD6-004: Legacy mesh-driven water shader flags parsed but not dispatched by the renderer

Labels: medium,tech-debt,water,nif,bug

**Description**: `BSWaterShaderProperty.water_shader_flags` (Displacement/LOD/Depth/Reflections/Refractions/Cubemap, per nif.xml) is captured end-to-end into the `Material` ECS component and versioned in the save format, but `grep -rn water_shader_flags byroredux crates/renderer` shows no read outside `material_translate.rs`'s own tests. Doc comment says renderer dispatch is "#977 follow-up work," still not wired. Distinct from the WATR-record-driven water system (audited under WATAL) — this is specifically NIF-mesh-authored water common on Oblivion/Skyrim exteriors. Closed issue #977 fixed the analogous `BSSkyShaderProperty` consumption ("Skyrim sky meshes render as magenta") but the water-property half of the same finding was not carried through.

**Evidence**:
`crates/nif/src/import/material/mod.rs:918-946` (`water_shader_flags`); threaded via `byroredux/src/material_translate.rs`; consumer gap confirmed in `byroredux/src/render/water.rs` / `crates/renderer/src/vulkan/water.rs` (no read of the flags).

**Impact**: NIF-mesh-authored water (common on Oblivion/Skyrim exteriors, `meshes/water/*.nif`) renders with a fixed default feature set regardless of authored per-mesh flags — fidelity-only unless the fixed default happens to diverge visibly from an authored disable of e.g. reflections.

**Related**: Regression of #977 (closed; fixed the sibling `BSSkyShaderProperty` consumption but not the `BSWaterShaderProperty` half of the same finding).

**Suggested Fix**: Confirm current visual behavior (fixed-default feature set regardless of authored flags would make this fidelity-only, not correctness), then either wire the per-flag dispatch or document the fixed-default behavior explicitly in `docs/engine/watal.md`.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser→`Material` boundary — never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
