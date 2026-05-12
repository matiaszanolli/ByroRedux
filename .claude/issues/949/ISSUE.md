# Issue #949 — REN-D4-NEW-03: gbuffer::initialize_layouts uses deprecated TOP_OF_PIPE with non-empty dst_access_mask

**Severity**: LOW
**Source audit**: docs/audits/AUDIT_RENDERER_2026-05-11_DIM4.md
**Labels**: documentation, renderer, vulkan, low

## Location

`crates/renderer/src/vulkan/gbuffer.rs:298-326` — `GBuffer::initialize_layouts` pipeline barrier.

## Evidence

`src_stage = TOP_OF_PIPE`, `dst_stage = FRAGMENT_SHADER | COMPUTE_SHADER`, `src_access = empty`, `dst_access = SHADER_READ`. Old layout `UNDEFINED`.

## Fix sketch

Strictly correct but Synchronization2 validation prefers `srcStageMask = NONE`. Pilot for Sync2 migration sweep — defer to bundle with other one-time-init barriers (taa.rs / svgf.rs / caustic.rs / ssao.rs `initialize_*` siblings).
