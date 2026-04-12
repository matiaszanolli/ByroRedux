# #259: R-04 — Command buffer/fence index indirection fragility

**Severity**: MEDIUM | **Domain**: renderer | **Type**: enhancement
**Location**: `crates/renderer/src/vulkan/context/draw.rs:49-93`
**Source**: `AUDIT_RENDERER_2026-04-12.md`

## Problem
Command buffers indexed by swapchain image count, fences by frame-in-flight count. The `images_in_flight` array bridges the gap correctly but makes the safety argument non-obvious.

## Fix
Align command buffers to frame-in-flight slots (like framebuffers already are).
