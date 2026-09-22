# SAFE-D1-2026-09-21-02: Launcher preflight leaks the `VkInstance` when `enumerate_physical_devices` fails

**Labels**: low, safety, tech-debt, bug

Filed via /audit-publish from docs/audits/AUDIT_SAFETY_2026-09-21.md.

**Severity**: LOW · **Dimension**: 1 — FFI lifetime
**Location**: `tools/byro-launcher/src/preflight.rs`: `probe()` (~:186-202)
**Status**: NEW (unchanged since `e05b4a9f8`, 2026-08-30; never reported)
**Verified against**: HEAD `f97775ca8`

## Description

In `probe()`, `instance.enumerate_physical_devices().map_err(|_| Blocker::NoAdapter)?` returns before `instance.destroy_instance(None)`, the function's only destroy call. The `VkInstance` created a few lines earlier leaks on that error path. This fails the safety audit's own check that an instance is destroyed on every path.

## Evidence

```rust
let instance = entry.create_instance(&create_info, None).map_err(|_| Blocker::NoVulkan)?;
let devices = instance
    .enumerate_physical_devices()
    .map_err(|_| Blocker::NoAdapter)?;   // returns without destroying `instance`
…
instance.destroy_instance(None);
best.ok_or(Blocker::NoAdapter)
```

## Impact

A one-shot `VkInstance` leak on the launcher's preflight error path, reached when a driver creates an instance but then fails enumeration. The process continues or exits. There is no per-frame cost.

## Related

None.

## Suggested Fix

Destroy the instance before propagating the error. Either match on the enumeration result, or wrap the instance in a small drop guard that calls `destroy_instance`.

Source: docs/audits/AUDIT_SAFETY_2026-09-21.md (SAFE-D1-2026-09-21-02)

## Completeness Checks
- [ ] **UNSAFE**: `probe()`'s SAFETY comment covers the destroy on the error path
- [ ] **SIBLING**: other `create_instance` users outside the renderer checked. Publish-time note: `crates/fsr3-sys/examples/vulkan_context_smoke.rs` has the same shape. `create_debug_utils_messenger(…)?` returns between `create_instance` and `destroy_instance` on its opt-in validation path (dev-only example).
