# #267: R2-01 — SSAO AO image cross-frame RAW hazard

**Severity**: HIGH | **Domain**: renderer, sync | **Type**: bug
**Location**: `crates/renderer/src/vulkan/ssao.rs:35-37`

## Problem
Single AO image shared across frame-in-flight slots. Frame N+1 fragment shader reads while frame N SSAO compute writes. RAW hazard / Vulkan spec violation.

## Fix
Duplicate ao_image/ao_image_view/ao_allocation to per-frame-in-flight arrays. ~2 MB cost at 1080p.
