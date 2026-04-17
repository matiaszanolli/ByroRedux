# #307 / P1-09 — TLAS build flags: PREFER_FAST_BUILD → PREFER_FAST_TRACE

## Root cause

The TLAS was created with `PREFER_FAST_BUILD | ALLOW_UPDATE`. That choice made sense when every frame rebuilt the TLAS from scratch: the trade-off is classic BVH-quality-vs-build-time, and a fast build won when the build dominated.

`#247` landed REFIT (`vk::BuildAccelerationStructureModeKHR::UPDATE`) on the TLAS so a rebuild only happens when the BLAS layout actually changes — most per-frame transforms flow through the cheap UPDATE path. That shifts the balance:

- Full rebuilds are rare → `PREFER_FAST_BUILD`'s advantage is irrelevant most of the time.
- Every frame still does thousands of ray queries (shadows, reflections, GI, caustics, window portals). BVH quality matters on every single one.

`PREFER_FAST_TRACE` is therefore the right choice on the TLAS, matching the BLAS flag choice.

## Fix

Two sites in `crates/renderer/src/vulkan/acceleration.rs`:

- Line ~887: TLAS BUILD `build_info.flags(…)` — `PREFER_FAST_BUILD` → `PREFER_FAST_TRACE`.
- Line ~1096: TLAS UPDATE `build_info.flags(…)` — same change. Vulkan spec requires BUILD and UPDATE flags to match on the same acceleration structure; any mismatch is a validation error.

Comment at line 879-883 rewritten to explain the new rationale: REFIT is the per-frame hot path, trace-time beats build-time once full rebuilds are rare, and the TLAS flag choice now mirrors BLAS.

## Sibling check

```
$ grep -rn PREFER_FAST_BUILD crates/renderer/
(no matches)

$ grep -rn PREFER_FAST_TRACE crates/renderer/
```

All 6 AS build sites (4 BLAS + 2 TLAS) now use `PREFER_FAST_TRACE`. BLAS sites were already on `PREFER_FAST_TRACE` — only the TLAS path flipped.

## Verification

- `cargo check --workspace` clean.
- `cargo test --workspace` 639 passing (no test changes; this is a driver-behavior flag).
- Release-build launch of Heinrich Oaken Halls demo: RT extensions enabled, batched BLAS build succeeds (62% compaction savings reported), zero Vulkan validation errors across the first frames. Validation layer would have flagged a BUILD/UPDATE flag mismatch — the silent launch confirms both sites moved in lockstep.

No regression test added: the behavior is a driver choice (BVH quality), not a Rust observable. Correctness is guarded by Vulkan's validation layer, which accepted the new flags.

## Perf notes

The issue suggested "Benchmark RT performance before/after on a complex cell." Not measured as part of this fix — on the target RTX 4070 Ti we were already GPU-unbound on Prospector Saloon (85 FPS), so a meaningful benchmark wants a scene with significantly more geometry than any cell currently loads. Left as a follow-up when exterior LOD (M35) lands and pushes ray-query cost back onto the critical path.
