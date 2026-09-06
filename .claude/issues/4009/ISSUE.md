# #4009 — REN-2026-09-06-D14-01: six rotted `file:NN` cross-references inside the caustic / water / volumetrics sources point at unrelated code

**Labels**: low, renderer, shaders, sync, water, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D14-01), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (comment accuracy; no runtime effect)
- **Dimension**: Caustics / Water / Volumetrics (in-code doc-rot)
- **Location**:
  - `crates/renderer/src/vulkan/water_caustic.rs` — module docstring, *"the caustic pipeline's pre-clear barrier at `caustic.rs:720-735`"*
  - `crates/renderer/shaders/caustic_splat.comp` — *"see draw.rs:268-273"*, *"`INSTANCE_FLAG_CAUSTIC_SOURCE` in `shader_constants_data.rs:86`"*, *"the Rust ↔ define lockstep assertion at shader_constants.rs:313-320"*
  - `crates/renderer/src/vulkan/volumetrics.rs` — two sites, both *"Mirrors `CausticPipeline::write_tlas` (caustic.rs:627)"*
  - `crates/renderer/shaders/water.frag` — *"the same Nperturbed already used by the primary refraction ray above (line ~547)"*
- **Status**: **NEW.** No open issue matches (searched `line number`, `line-number`, `doc-rot` + the file names). The adjacent `crates/renderer/src/vulkan/context/resize.rs` *"matches init behaviour at mod.rs:1422-1426"* is the same class in a Dim-16-adjacent file and is included in the fix scope below.
- **Description**: Each of these was correct when written and now names a different construct. Verified individually against HEAD:

  | Cited | Claimed to be | What is actually there |
  |---|---|---|
  | `caustic.rs:720-735` | the pre-clear barrier | the `write_combined_image_sampler` / `write_storage_buffer` block inside `write_descriptor_sets` |
  | `draw.rs:268-273` | the `sceneFlags.x` RT gate | a `morph_slot_backs_mesh` unit-test assertion |
  | `shader_constants_data.rs:86` | `INSTANCE_FLAG_CAUSTIC_SOURCE` | `VERTEX_STRIDE_FLOATS` (the real definition is ~380 lines further down) |
  | `shader_constants.rs:313-320` | the Rust↔`#define` lockstep assertion | the GLSL tokenizer's `while index < bytes.len()` loop |
  | `caustic.rs:627` (×2) | `CausticPipeline::write_tlas` | the image-view-creation error arm inside `create_slot` (`write_tlas` is ~166 lines later) |
  | `water.frag` "line ~547" | the primary refraction ray | `foamShoreline`'s `sceneFlags.x` early-out |
  | `context/mod.rs:1422-1426` | the bloom-init hard-fail | the `skin_first_sight_builds_scratch` field declaration |

  All seven still resolve to *plausible-looking* code, which is what makes them costly: a reader who follows one lands somewhere real and draws the wrong conclusion rather than noticing the reference is dead.
- **Evidence**: `sed -n` at each cited range, reproduced in the table above. `grep -n "pub fn write_tlas" crates/renderer/src/vulkan/caustic.rs` → line 793, not 627. `grep -n "INSTANCE_FLAG_CAUSTIC_SOURCE" crates/renderer/src/shader_constants_data.rs` → line 469, not 86.
- **Impact**: Auditor and maintainer time only — but this is the exact class the audit skill's *"Symbols, not line numbers — line anchors rot on every refactor"* rule exists to prevent, and the rule is currently enforced only on audit skill files (`_audit-validate.sh`) and not on production comments. Two of the seven sit in `caustic_splat.comp`, a file that changed three times in two days.
- **Related**: #1114 (the path-reference convention), the `_audit-validate.sh` gate, #3866 / #3842 / #3846 (the same doc-rot family in the acceleration and bindings docs).
- **Suggested Fix**: Replace each with the symbol it means — `CausticPipeline::clear_for_skip` / the moving-camera arm of `CausticPipeline::dispatch`; the `patch_camera_rt_flag` site in `draw_frame`; the bare constant name `INSTANCE_FLAG_CAUSTIC_SOURCE` plus the two tests that actually pin it (`caustic_splat_comp_uses_named_instance_flag_constant` and `instance_flag_bits_match_scene_buffer_consts`, both in `crates/renderer/src/shader_constants.rs`); `CausticPipeline::write_tlas`; `traceWaterRay`'s refraction call; `VulkanContext::new`'s bloom-init arm. Cheap, and it is the same rule the audit tooling already enforces one directory over.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **SPIRV**: If GLSL changed, recompile the `.spv` (`cd crates/renderer/shaders && glslangValidator -V -I. <shader> -o <shader>.spv`) and commit it
- [ ] **TESTS**: A regression test pins this specific fix
