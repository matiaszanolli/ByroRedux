# #6968: TD3-2026-11-15-03: `shader-pipeline.md` binding table omits Set 0 binding 2 (`TextureFirstLevels`)

- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/6968
- **Labels**: low,renderer,shaders,documentation,doc-rot,tech-debt
- **Source**: docs/audits/AUDIT_TECH_DEBT_2026-11-15.md

From the tech-debt audit `docs/audits/AUDIT_TECH_DEBT_2026-11-15.md` (HEAD 995ed089b).

- **Severity**: LOW. **Dimension**: 3. **Effort**: trivial. **Labels**: `tech-debt`, `doc-rot`. **Existing**: none.
- **Age**: f754eee28 (2026-11-14, #6629) added the binding and updated `texture-streaming.md` and `memory-budget.md`, not the table.
- **Evidence**: `crates/renderer/shaders/include/bindings.glsl:16` (`layout(std430, set = 0, binding = 2) readonly buffer TextureFirstLevels`), pinned at `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:8183`. `docs/engine/shader-pipeline.md` lists Set 0 bindings 0 and 1 only (rows at ~856-857) and never names `TextureFirstLevels`. I diffed every other Set 0/1 binding in `bindings.glsl` against the table: all present (12 and 18 are declared outside `bindings.glsl` and are in the table).
- **Fix**: add the row `| 0 | 2 | STORAGE_BUFFER | TextureFirstLevels (per-handle first resident mip, #6629) | triangle, water, groundcover_blade |`. The Set-1 lockstep guards do not cover Set 0, so nothing else will catch the next one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
