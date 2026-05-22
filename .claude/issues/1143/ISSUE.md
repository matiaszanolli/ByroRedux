title:	PERF-D1-NEW-01: volumetric dispatch gate is a runtime if on a host const — verify DCE under RenderDoc
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, performance, vulkan
comments:	0
assignees:	
projects:	
milestone:	
number:	1143
--
## Finding

**ID**: PERF-D1-NEW-01
**Severity**: LOW
**Dimension**: GPU Pipeline (Dim 1)
**Audit**: AUDIT_PERFORMANCE_2026-05-16.md
**Location**: `crates/renderer/src/vulkan/context/draw.rs:1410, 2191`

## Description

`VOLUMETRIC_OUTPUT_CONSUMED` is a `const bool` set to `false` (`volumetrics.rs:124`). Two call sites read it as a runtime condition:

```rust
if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED { ... }  // draw.rs:1410, 2191
```

The const-bool gate works (the compiler dead-code-eliminates the inner branch when `false`), but the *enclosing if-let chain* may not receive the same DCE treatment if the compiler can't prove `self.volumetrics.is_some()` is the only check protecting state needed by the false branch. This is the addressed-but-not-optimal follow-up to the 2026-05-10 audit's PERF-GP-01.

## Impact

Currently un-measured. If LLVM does fold this out the impact is zero. If not, ~10–20 ms/frame on cells with TLAS (the original PERF-GP-01 magnitude). **Recommendation**: verify under RenderDoc / Nsight before fixing.

## Suggested Fix

Replace the runtime `if` with a `#[cfg(feature = "volumetrics")]` Cargo feature (flipped to `true` when M-LIGHT v2 lands) so the dispatch site disappears from the binary entirely. This guarantees DCE regardless of optimizer heuristics.

Alternative: lift the const to a `const fn` and gate at compile time.

~5 LOC change.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Check both draw.rs call sites (1410 and 2191) are updated in lockstep
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: A `#[cfg]` gate must be verified with `cargo check --features volumetrics` once the feature name is chosen

**Related**: PERF-GP-01 (2026-05-10 audit, closed by adding the `VOLUMETRIC_OUTPUT_CONSUMED` const). `#928` — the future flip that will turn volumetrics on.
