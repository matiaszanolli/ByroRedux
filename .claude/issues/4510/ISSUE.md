# REN-D4-2026-09-20-01: water pipeline push-constant block is 28 B in GLSL against a 16 B declared range — two VUID-layout-10069 errors fire on every session start

- **ID**: REN-D4-2026-09-20-01
- **Labels**: high,renderer,vulkan,pipeline,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: HIGH · **Dimension**: Pipeline/RenderPass
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D4-2026-09-20-01)

**Location**: `crates/renderer/shaders/water.vert:163-166` + `water.frag:138-141` (`WaterDrawPush { uint waterIndex; uvec3 _reserved; }`); `crates/renderer/src/vulkan/water.rs:473-477` (range = size_of::<WaterPush>() = 16), `:198-203` (const-assert), `:784-793` (16 B pushed)

**Description**
Under std430 push-constant layout the uvec3 member aligns to 16, so the GLSL block spans [0,28] while the pipeline layout's VkPushConstantRange is [0,16]. The const-assert pins the wrong invariant: only the Rust struct is 16 B, not the shader block.

**Evidence**
Reproduced this session: `BYRO_VALIDATION=1 ./target/release/byroredux --cornell --bench-frames 45` (RTX 4070 Ti) emits exactly two validation errors at water-pipeline creation (vertex+fragment stages, block range [0,28] outside range [0,16]), immediately followed by the `Water pipeline created (... 16B push selector ...)` log. The #3572 commit message (`ba4c0efcf`) is the only prior record — no issue tracked them.

**Impact**
A compliant driver may reject pipeline creation or return garbage for the out-of-range word; today's driver tolerates it because `_reserved` is never read. Two ERROR-level lines every session start also desensitize the validation signal.

**Suggested Fix**
Shrink the GLSL block to the real 16 B selector (drop `_reserved` or repack), fix the const-assert message, keep the 16 B push; pin the block size via SPIR-V reflection rather than Rust size_of alone.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix
