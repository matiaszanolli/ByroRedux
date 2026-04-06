# Renderer Audit — 2026-04-05b

**Auditor**: Renderer Specialist (Claude Opus 4.6)
**Scope**: Full Vulkan renderer — synchronization, GPU memory, pipeline, render pass, commands, lifecycle

---

## Executive Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0     |
| MEDIUM   | 3     |
| LOW      | 5     |
| INFO     | 3     |

The renderer is in excellent shape. All 15 HIGH/MEDIUM issues from the 2026-04-04 audit
have been resolved. No spec violations or correctness bugs remain. Findings are performance
edge cases and defensive hardening opportunities.

---

## Findings

### REN-001: Depth attachment store op STORE is wasteful
**Severity:** LOW | **Location:** helpers.rs:57 | **Status:** NEW
Depth uses STORE but is never read by subsequent passes. Wastes bandwidth on tile-based GPUs.
**Fix:** Change to DONT_CARE unless depth readback is planned.

### REN-002: Swapchain raw pointer bypasses builder lifetime
**Severity:** MEDIUM | **Location:** swapchain.rs:71-88 | **Status:** NEW
Raw `p_queue_family_indices` pointer from stack-local array. Currently safe but not
lifetime-checked. Future refactor could create dangling pointer.
**Fix:** Use ash builder `.queue_family_indices()`.

### REN-003: Pipeline cache loaded from CWD without validation
**Severity:** MEDIUM | **Location:** helpers.rs:260-286 | **Status:** NEW
Relative path, no header validation. Stale/corrupted cache from different GPU/driver.
**Fix:** Absolute path + validate 32-byte Vulkan cache header before use.

### REN-004: Mixed descriptor set indexing (frame vs image)
**Severity:** LOW | **Location:** draw.rs:197 vs draw.rs:232 | **Status:** NEW
Scene sets indexed by frame-in-flight, texture sets by swapchain image. Not a bug but
a maintenance hazard.
**Fix:** Add comment explaining the asymmetry.

### REN-005: TLAS resize still causes full GPU stall
**Severity:** MEDIUM | **Location:** acceleration.rs:271-278 | **Status:** EXISTING
device_wait_idle on TLAS capacity exceed. 4096 initial mitigates but large exteriors stall.
**Fix:** Deferred TLAS destruction via ring buffer, or proactive resize at frame start.

### REN-006: Shader modules retained for context lifetime
**Severity:** LOW | **Location:** context/mod.rs:55-58 | **Status:** EXISTING
Modules serve no purpose after pipeline creation.
**Fix:** Destroy immediately after initial pipeline creation.

### REN-007: No viewport/scissor re-set after UI pipeline bind
**Severity:** LOW | **Location:** draw.rs:281-285 | **Status:** NEW
Currently correct (inherits prior state). Fragile if future variant changes viewport.
**Fix:** Re-set or document intentional inheritance.

### REN-008: Depth bias set for every draw call including non-decals
**Severity:** LOW | **Location:** draw.rs:246-252 | **Status:** NEW
Redundant (0,0,0) state update for non-decal geometry.
**Fix:** Track last-set bias, skip when unchanged.

### REN-009: Texture deferred destruction keyed on call count not frame count
**Severity:** INFO | **Location:** texture_registry.rs:246 | **Status:** NEW
Assumes update_rgba called at most once per frame per handle.
**Fix:** Document assumption or track frame numbers.

### REN-010: Swapchain uses raw struct initialization
**Severity:** INFO | **Location:** swapchain.rs:71-88 | **Status:** NEW
Inconsistent with rest of codebase using builder pattern.

### REN-011: No anisotropic filtering on shared sampler
**Severity:** INFO | **Location:** texture_registry.rs:89 | **Status:** NEW
Significantly improves texture quality at oblique angles. All desktop GPUs support >= 16x.
**Fix:** Enable 8x/16x anisotropic filtering.

---

## All Previous Issues Verified Fixed

R-1 (flush), R-2 (TLAS barrier), R-3 (present mutex), R-4 (swapchain handoff),
R-5/6 (AS DEVICE_LOCAL), R-7 (texture destroy order), R-8 (TLAS pre-size),
R-11 (one-time fence), R-14 (backface cull), R-15 (normal NaN guard),
R-16 (LATE_FRAGMENT_TESTS), R-17 (depth format query), R-18 (host writes),
R-21 (UI depth bias), R-23 (pipeline layout reuse).
