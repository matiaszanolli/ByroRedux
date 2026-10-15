# TD7-2026-10-15-06: The material "four decal slots" count is an independent literal in six places and two shader path-tracing ceilings sit outside the budget table

Labels: low,renderer,shaders,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD7-2026-10-15-06`).

- **Severity**: LOW
- **Dimension**: 7 - Magic Numbers & Hardcoded Constants (shader provenance)
- **Location**: `crates/renderer/shaders/include/ray_hit.glsl:441,516`, `crates/renderer/shaders/triangle.frag:381` (`decalIndex < 4`, beside `materialDecals[4]` built from `decalMap0Index..decalMap3Index`), `crates/nif/src/import/types.rs:396` (`decals: [T; 4]`), `crates/nif/src/blocks/properties.rs:388` (`const MAX_DECAL_SLOTS: u32 = 4`); shader-local `const int MAX_GLASS_INTERFACES = 4` (`shaders/include/shadow_transport.glsl:99`) and `const int MAX_DIFFUSE_BOUNCES = 2` (`shaders/triangle.frag:4347`)
- **Status**: NEW
- **Age**: decals `31ebdd7ca` / `1d94eb246` / `1c8b39644` (older than 2026-08); `MAX_GLASS_INTERFACES` `1fb79038a`; `MAX_DIFFUSE_BOUNCES` `6c56e3115`
- **Effort**: small
- **Description**: `shader_constants_data.rs` holds `MAX_PATH_SEGMENTS`, `MAX_SHADED_HITS`, `MAX_DIRECT_SHADOW_SAMPLES` and `GLASS_RAY_BUDGET` for the same reason (#3879: a hand-typed loop bound clamps a CPU-side tier increase to a silent no-op). The decal-slot count has four named struct fields on the GPU side and no constant, and the two path-tracing ceilings above are named but shader-local. No misbehaviour found: the decal count is pinned by the four-field `GpuMaterial` layout, and no CPU-side value is clamped by the two ceilings. One doc discrepancy to settle when it is next touched: the glass telemetry comment at `shader_constants_data.rs:1134-1138` says the interface allowance grows "from 2 to 8" per tier, while the shadow-transport loop is fixed at 4.
- **Evidence**: `for (int decalIndex = 0; decalIndex < 4; ++decalIndex)` in three shader files; `grep -n 'DECAL' crates/renderer/src/shader_constants_data.rs` finds only `RENDER_LAYER_DECAL`.
- **Impact**: none today; the same lockstep class #3879/#3880 closed, one level down.
- **Related**: #3879, #3880, `feedback_shader_struct_sync`.
- **Suggested Fix**: `MATERIAL_DECAL_SLOTS` (Rust `MaterialTextureSet::decals.len()`-derived) emitted through the generated header and used by the three GLSL loops; move the two path ceilings into the table or add a comment tying them to the budget tiers.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
