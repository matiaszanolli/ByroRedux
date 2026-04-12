# #268: R2-02 — SVGF denoises albedo-baked indirect

**Severity**: MEDIUM | **Domain**: renderer | **Type**: enhancement
**Location**: `crates/renderer/shaders/triangle.frag:666-671`

## Problem
Indirect lighting has albedo baked in before SVGF. Denoiser blurs texture detail. Documented design debt — will become visible with spatial filtering.

## Fix
Demodulate albedo before SVGF, remodulate in composite pass.
