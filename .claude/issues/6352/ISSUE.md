# TD3-2026-10-25-01: GPU-struct size claims in prose are stale outside `pinned_sizes()` (ReSTIR `Reservoir` 32 to 36 B; `GpuTerrainTile` "208-byte" stride)

Labels: medium, renderer, shaders, doc-rot, tech-debt, documentation
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6352
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: MEDIUM (a stale GPU size in prose; severity table)
- **Dimension**: 3
- **Effort**: trivial
- **Location**: `crates/renderer/shaders/include/bindings.glsl:815`, `crates/renderer/shaders/triangle.frag:3392`, `crates/renderer/src/vulkan/restir.rs:859` (assert message), `crates/renderer/shaders/include/bindings.glsl:529`
- **Age**: #6250 `bed162262` (2026-10-24) grew `Reservoir` 32 to 36 B; #6197 `b81a43443` (2026-10-22) grew `GpuTerrainTile` 208 to 224 B
- **What**:
  - `bindings.glsl:815` says "32 B/reservoir". `triangle.frag:3392` says "would not fit the 32-byte reservoir". The `restir.rs:859` assert message says "existing 32-byte reservoir stride".
  - The live values are `RESERVOIR_STRIDE = 36` (`restir.rs:44`), pinned by `reservoir_stride_matches_shader`, and the struct's own comment at `bindings.glsl:838` ("std430 stride 36 B"). `memory-budget.md:277` is already correct.
  - `bindings.glsl:529` says the tail pads "round the record out to the 208-byte std430 array stride". The next lines (`:545`) and `gpu_terrain_tile_is_224_bytes` say 224.
  - `gpu_material_size_claims` cannot see either: `Reservoir` is not in `pinned_sizes()`, and the terrain claim does not name the type.
- **Fix**: Edit the four sites to 36 B / 224-byte. Optionally add `Reservoir` (36) to `pinned_sizes()` or to a second guard that greps `\b32[- ]?(B|byte)\b` near "reservoir". The audit-renderer skill line 75 ("reservoir stays 32 B") needs the same fix (Dim 4 hand-off).
- **Checked clean in the same sweep**: `Vertex` (104 B, ~15 sites), `GpuWaterParams` (368, 6 sites), `GpuRayBudget` (68), `GpuFogVolume` (96, header 16 + 512x96 = 49168), `GpuCombustionLightMoment` (32), `GpuFogClusterEntry` (16), `GpuSelectedRayProbe` (144), `SkinPushConstants` (32), `WaterPush` (16), `UiVertex` (20), the `memory-budget.md` bone/instance/material figures (`MAX_SKIN_SLOTS` 1364, `MAX_INSTANCES` 262144, `MAX_MATERIALS` 16384, `VERTEX_POOL_SOFT_CAP` 4M x 104).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD3-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
