# PERF-D4-01: upload_lights is the one per-frame SSBO without a content-hash dirty gate

**Labels**: low, performance, bug

**Severity**: LOW
**Dimension**: SSBO Sizing & Upload
**Source**: `docs/audits/AUDIT_PERFORMANCE_2026-07-16.md`

## Location
`crates/renderer/src/vulkan/scene_buffer/upload.rs:19-84`

## Description
Instances (#1134), materials (#878), and indirect draws (#1809) all gained a content-hash dirty-gate skip; `upload_lights` did not. Blast radius is small (light buffers are a few KB/frame) and the gate would frequently miss anyway on flickering-torch content — a consistency/hardening gap, not a hot-path cost.

Verified current: `upload_lights` (`crates/renderer/src/vulkan/scene_buffer/upload.rs:19-84`) still writes directly to mapped GPU memory every call with no preceding content-hash comparison, unlike the sibling instance/material/indirect-draw upload paths.

## Suggested Fix
Add a content-hash gate mirroring the instances/materials/indirect-draws pattern, understanding it will often miss on flickering-light content (low value, but consistent with the established pattern).

## Completeness Checks
- [ ] **SIBLING**: Mirror the content-hash dirty-gate pattern already applied to instances (#1134), materials (#878), indirect draws (#1809)
- [ ] **TESTS**: A regression test pins this specific fix if implemented
