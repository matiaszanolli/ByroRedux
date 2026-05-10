---
issue: 0
title: REN-D5-NEW-03: First batch always emits cmd_set_cull_mode(BACK) — wasted state change for two-sided meshes
labels: renderer, medium, vulkan, performance
---

**Severity**: MEDIUM (wasted state change per frame)
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-09.md (Dim 5)

## Location

- `crates/renderer/src/vulkan/context/draw.rs:1287` — unconditional `cmd_set_cull_mode(BACK)`

## Why it's a bug

`cmd_set_cull_mode(BACK)` at draw.rs:1287 is unconditional — wasted when first batch is two-sided. For exterior cells where the first draw is often vegetation/foliage (two-sided), every frame issues a redundant state change.

## Fix sketch

Switch to the `Option<CullModeFlags>` sentinel pattern already used for `last_render_layer` / `last_z_function`:

```rust
let mut last_cull: Option<vk::CullModeFlags> = None;
for batch in &batches {
    let want_cull = if batch.two_sided { vk::CullModeFlags::NONE } else { vk::CullModeFlags::BACK };
    if last_cull != Some(want_cull) {
        unsafe { device.cmd_set_cull_mode(cmd, want_cull) };
        last_cull = Some(want_cull);
    }
    // ... rest of batch
}
```

## Completeness Checks

- [ ] **UNSAFE**: No unsafe change.
- [ ] **SIBLING**: Verify `last_render_layer` and `last_z_function` use the same Option pattern.
- [ ] **TESTS**: Visual regression check; capture should be byte-identical pre/post.

🤖 Filed by /audit-publish from docs/audits/AUDIT_RENDERER_2026-05-09.md
