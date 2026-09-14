# #4382 — TD8-003: Six dead `pub fn`s on engine-consumed types

**Labels**: low, renderer, scripting, animation, tech-debt, bug
**Filed from**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md`
**URL**: https://github.com/matiaszanolli/ByroRedux/issues/4382

- **Severity**: LOW · **Dimension**: 8
- **Location**: `crates/renderer/src/vertex.rs:110` (`Vertex::new_skinned`), `crates/renderer/src/vulkan/texture.rs:500` (`Texture::from_dds`), `crates/renderer/src/vulkan/frame_upscaler.rs:330` (`FrameUpscaler::output_image`), `crates/renderer/src/vulkan/context/mod.rs:2214` (`VulkanContext::with_transfer_commands`), `crates/scripting/src/scene.rs:85` (`SceneActorBindings::unbind`), `crates/core/src/animation/stack.rs:358` (`collect_stack_text_events`) · **Status**: NEW · **Age**: 1.5–5.5 months · **Effort**: small · **Kind**: tech-debt
- **Finding**: Each name occurs only at its definition. `with_transfer_commands`'s doc says "prefer this over the free function", but the free functions have 51 call sites and the method has none. `collect_stack_text_events` is "retained for test ergonomics" but no test uses it. Eleven more zero-reference items in library crates are noted in Dim 8's scratch report, not filed; `read_lstring_sub` (#1045) has never had an adopter and deserves a look.
- **Suggested Fix**: Delete all six. Removing the six-line method also shrinks `context/mod.rs` (#4217).

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-14.md` (HEAD `358999c40`)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shaders / block parsers / skill files / docs)
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
