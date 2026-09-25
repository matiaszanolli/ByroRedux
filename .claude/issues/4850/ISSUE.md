# #4850 — REN-D3-2026-09-24-05: the sixth `GpuInstance` copy (`groundcover_models.comp`) is missing from the #3231 vec3 guard, which also has no discovery leg

**Labels**: bug,renderer,low,shaders,test-gap
**Filed from**: docs/audits/AUDIT_RENDERER_2026-09-24.md (audited `main` @ `6c5555c70`)

- **Severity**: LOW (test gap; the copy uses only scalars and `uvec2`)
- **Dimension**: GPU-Struct Layout
- **Location**: `scene_buffer/shader_contract_tests.rs` — `gpu_instance_glsl_declarations_never_use_a_3_component_vector_type` (5 sources, no `assert_mirror_list_is_complete`) and `every_shader_struct_gpu_instance_names_expected_fields` (5 sources).
- **Status**: NEW (the copy landed in `aabd99a05`; `gpu_instance_glsl_copies_stay_in_lockstep` was updated, its siblings were not)
- **Description**: The vec3 rule is the one whose violation produced a silent device-lost hang (#3231). The lockstep test compares field *names* only, so a type change in the new copy's tail lanes (three scalars -> `uvec3`) keeps names and order identical and passes; only the vec3 test would catch it, and it does not scan that file.
- **Suggested Fix**: Add `groundcover_models.comp` to both lists and give the vec3 test the same `assert_mirror_list_is_complete("struct GpuInstance", …)` leg the lockstep test has.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders, other producers/consumers, other games)
- [ ] **TESTS**: A regression test pins this specific fix (and fails when the guarded code is deleted — mutation-check it)
