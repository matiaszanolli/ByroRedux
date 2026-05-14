# REN-D10-NEW-06: composite render-pass external dep lacks UNIFORM_READ — defence-in-depth gap

**State**: OPEN
**Labels**: renderer, low, vulkan, sync

**Source**: `docs/audits/AUDIT_RENDERER_2026-05-11_DIM10.md`
**Dimension**: Denoiser & Composite
**Severity**: LOW
**Confidence**: MED

## Observation

`crates/renderer/src/vulkan/composite.rs:404-415`:

```rust
let composite_dep_in = vk::SubpassDependency::default()
    .src_subpass(vk::SUBPASS_EXTERNAL)
    .dst_subpass(0)
    .src_stage_mask(
        vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
            | vk::PipelineStageFlags::COMPUTE_SHADER,
    )
    .src_access_mask(
        vk::AccessFlags::COLOR_ATTACHMENT_WRITE | vk::AccessFlags::SHADER_WRITE,
    )
    .dst_stage_mask(vk::PipelineStageFlags::FRAGMENT_SHADER)
    .dst_access_mask(vk::AccessFlags::SHADER_READ);
```

The composite incoming subpass dependency declares `dst_access_mask = SHADER_READ` only. UNIFORM_READ on the composite params UBO is not enumerated — that execution dependency is currently covered by the bulk pre-render barrier emitted in `draw.rs` (#909 / REN-D1-NEW-03).

## Why it's a bug

Validation-clean today because #909's bulk barrier covers the UBO host-write → fragment-uniform-read dependency. If someone removes or restructures the bulk barrier so composite is omitted, the render-pass external dep wouldn't pick up the UBO read on its own and a HOST→FRAGMENT hazard could re-surface.

Defence-in-depth gap, not a live bug.

## Suggested fix

Add `vk::AccessFlags::UNIFORM_READ` to `composite_dep_in.dst_access_mask` so the render-pass dependency stands on its own without relying on #909's bulk barrier. The composite descriptor set binds the params UBO at set 0 binding 3 (`composite.rs:471-520`); adding UNIFORM_READ matches that consumer.

## Completeness Checks
- [ ] **UNSAFE**: No new unsafe.
- [ ] **SIBLING**: Check other render-pass external deps (main G-buffer pass) for similar gaps. The main pass also reads a camera UBO; its external dep at `helpers.rs:create_render_pass` should be cross-checked.
- [ ] **DROP**: N/A.
- [ ] **LOCK_ORDER**: N/A.
- [ ] **FFI**: N/A.
- [ ] **TESTS**: N/A.

## Dedup

- No existing OPEN issue matches.
- Adjacent to #909 invariant (closed); this finding hardens against a future #909 revert/restructure.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
