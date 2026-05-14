# TD4-002: MAX_FRAMES_IN_FLIGHT constant duplicated

**Severity**: HIGH
**Domain**: sync, renderer, vulkan
**Audit**: docs/audits/AUDIT_TECH_DEBT_2026-05-13.md
**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/1037

## Locations
- `crates/renderer/src/vulkan/sync.rs:6` (canonical)
- `crates/renderer/src/texture_registry.rs:28` (private duplicate)

## Fix
Delete texture_registry copy, `use super::vulkan::sync::MAX_FRAMES_IN_FLIGHT;`.

## Why HIGH
sync.rs:28 explicitly documents the upgrade path to 3. Drift = silent UAF of freed texture descriptors. Invisible to cargo test.
