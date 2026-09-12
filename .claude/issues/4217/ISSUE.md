# TD1-001: `context/mod.rs` still mixes telemetry-fill methods and pure data types with its lifecycle-phase split axis

Labels: low,tech-debt,renderer,bug

**Description**: prod_loc confirmed unchanged at exactly 2831 (file total 3470 including tests). Two concerns don't fit the existing 17-submodule lifecycle-phase split: (1) 5 telemetry query methods (`fill_upscaler_telemetry`, `fill_scratch_telemetry`, `fill_skin_coverage_stats`, `fill_rt_integrity_stats`, `fill_shadow_mask_census`, ~392 lines) that read internal state into debug-UI/ECS stats structs, none touching draw/init/resize logic; (2) pure data types (`DrawCommand::to_gpu_material`/`material_hash`, `SkyWeatherParams`/`DofView`/`SkyParams` defaults, `ScreenshotHandle`, ~770 lines) that never reference `VulkanContext` itself. No function here exceeds 200 LOC by accurate brace-depth measurement. Related to OPEN #3736 (the broader God-Object framing of `VulkanContext` itself) but distinct in scope — this finding proposes a concrete, narrow extraction.

**Evidence**:
`crates/renderer/src/vulkan/context/mod.rs:2309-2701` (telemetry fillers), `:430-1202` (data types).

**Impact**: No runtime impact — pure maintainability. Extraction removes ~1160 of 2831 prod_loc, dropping the file well under 2000 without touching Vulkan lifecycle/drop-order code.

**Related**: OPEN #3736 (`VulkanContext` God Object, broader scope).

**Suggested Fix**: Extract the 5 `fill_*` methods into a new `mod telemetry;` and the data types into a new `mod types;` — both parallel to the existing `render_debug`/`resources` submodules.



## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_TECH_DEBT_2026-09-11.md`.*
