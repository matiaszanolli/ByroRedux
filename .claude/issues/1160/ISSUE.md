# #1160 — REN-D10-NEW-13: Composite outgoing render-pass dependency uses deprecated BOTTOM_OF_PIPE (DST side)

**Severity**: LOW
**Domain**: renderer
**Status**: OPEN
**Source Audit**: `docs/audits/AUDIT_RENDERER_2026-05-17_DIM9_DIM10.md` — Dimension 10

## Location

`crates/renderer/src/vulkan/composite.rs:448`

## Description

The outgoing subpass dependency (`composite_dep_out`, subpass 0 → SUBPASS_EXTERNAL) sets `dst_stage_mask(BOTTOM_OF_PIPE)` with `dst_access_mask(empty)`. This is the legacy "release ownership / no further synchronization required" idiom. Vulkan 1.3 deprecated `BOTTOM_OF_PIPE` and `TOP_OF_PIPE` in favour of `vk::PipelineStageFlags::NONE`.

The dual closeouts already happened on the SRC side (#949 / #1100 / #1121 / #1122 migrated 8+ `TOP_OF_PIPE` source masks to `NONE`); this site is the matching DST-side leftover. SVGF's own `initialize_layouts` at `svgf.rs:753` already uses `NONE` for the same scenario — composite is the odd one out.

## Evidence

```rust
let composite_dep_out = vk::SubpassDependency::default()
    .src_subpass(0)
    .dst_subpass(vk::SUBPASS_EXTERNAL)
    .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
    .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
    .dst_stage_mask(vk::PipelineStageFlags::BOTTOM_OF_PIPE)  // ← deprecated
    .dst_access_mask(vk::AccessFlags::empty());
```

Sibling already-correct site:

```rust
// svgf.rs:753 — initialize_layouts UNDEFINED → GENERAL
device.cmd_pipeline_barrier(
    cmd,
    vk::PipelineStageFlags::NONE,
    vk::PipelineStageFlags::COMPUTE_SHADER,
    ...
);
```

## Suggested Fix

Migrate to `vk::PipelineStageFlags::NONE`. One-line change, matches the sibling `initialize_layouts` site in SVGF / SSAO / caustic.

## Related

- #1121 / `a49eb945` — six `TOP_OF_PIPE` → `NONE` migrations on the SRC side
- #1122 — TLAS count invariant test + sibling sites
