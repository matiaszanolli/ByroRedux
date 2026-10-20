# TD2-2026-10-19-01: Eight `initialize_layouts` bodies are the same UNDEFINED→GENERAL one-time submit, and their sixteen callers all carry the "nothing has referenced these images" destroy premise that #6063 just proved false for the one copy it found

Labels: medium,renderer,sync,tech-debt,bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-19.md

- **Severity**: MEDIUM (duplicated logic, one copy fixed and the siblings not — severity table row "divergent bug-fix history")
- **Dimension**: 2 - Logic Duplication
- **Location**: bodies `vulkan/bloom.rs:536`, `taa.rs:563`, `svgf.rs:1005`, `water_caustic.rs:151`, `caustic.rs:798`, `gbuffer.rs:331`, `shadow_filter.rs:279`, `volumetrics/init.rs:914` (the last two take `GpuUploadCtx` / extra staging); callers `context/init.rs:848,1097,1158,1223,1320,1391,1526`, `context/resize.rs:621,820,915,977,1674` and `taa.rs:818`, `svgf.rs:1431`, `caustic.rs:1190`
- **Age**: copies since #949/#1100 (2026-05); barrier helper funnel `13e00a24b` (2026-09-15) and #4221 shared the barrier but not the submit; `4a6128389` #6063 (2026-10-19, 18:37) fixed only `shadow_filter`
- **Effort**: small (helper) / small (caller policy)
- **Description**: seven of the eight are: `with_one_time_commands(...)`, collect `image_barrier_undef_to_general(img)` for the pass's images, one `cmd_pipeline_barrier(NONE -> COMPUTE_SHADER, ...)`. Only the destination stage (water_caustic: FRAGMENT; gbuffer: UNDEF->SHADER_READ) and the image list differ. `caustic.rs:803-813` still hand-builds the barrier (layered range) after #4221 funnelled the rest.
- **Evidence**: #6063's message: "destroyed the pass's images on any failure, including a post-submit fence-wait error from the UNDEFINED -> GENERAL layout walk, which may still be executing against them"; it split the walk out and added the `OneTimeCommandError::may_be_in_flight` leak arm (`shadow_filter.rs:96`, `resize.rs:737`) for that one pass. The sibling callers still `destroy` straight after a failed `initialize_layouts` under the comment "no frame command buffer has yet referenced its images": `init.rs:1223-1235` (SVGF), `init.rs:1320-1329` (caustic), `init.rs:1526-1535` (TAA), `init.rs:848-859` (water caustic), `resize.rs:1674-1681` (TAA, "nothing has referenced these images"). That premise is exactly what #6063 showed false for a `MaybeInFlight` error. (Bug-class ownership: renderer/concurrency audit; filed here because the cause is the unshared wrapper.)
- **Impact**: the in-flight-leak policy (#4891 / #5947 / #6063) has to be re-applied per pass; it has now been applied to 1 of 8, and the remaining 7 free GPU memory a submitted command buffer may still touch.
- **Related**: #6063, #4891, #5947, #4221, #949/#1100
- **Suggested Fix**: `fn initialize_layouts_once(ctx: GpuUploadCtx, images: impl IntoIterator<Item = vk::Image>, dst_stage: vk::PipelineStageFlags) -> Result<(), OneTimeCommandError>` beside `image_barrier_undef_to_general` in `vulkan/descriptors.rs` (or `texture.rs` next to `with_one_time_commands`); the eight bodies shrink to an image list. Then one `fn destroy_unless_in_flight(pass, err)` (the #6063 arm) replaces the sixteen caller blocks, and a source-scan test pins "no `initialize_layouts` caller destroys without consulting `may_be_in_flight`", modelled on the #6063 test at `shadow_filter.rs:485`.

Related: #6063 (closed; fixed the shadow-filter copy only), #4891, #5947, #4221.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._
