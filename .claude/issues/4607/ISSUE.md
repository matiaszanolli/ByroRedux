# PERF-D1-2026-09-21-01: Ground-cover host collection rebuilds std SipHash maps/sets and ~10 fresh allocations every exterior frame

**Labels**: bug, medium, performance, terrain-exterior

Filed via /audit-publish from docs/audits/AUDIT_PERFORMANCE_2026-09-21.md.

**Severity**: MEDIUM (redundant allocation and hashing on the per-frame render path; the #2923 rule class) · **Dimension**: 1 — CPU Hot Paths
**Location**:
- `byroredux/src/render/groundcover.rs:14`: `use std::collections::{HashMap, HashSet};`
- `byroredux/src/render/groundcover.rs:112-181`: `GroundCoverResidency::reconcile`
- `byroredux/src/render/groundcover.rs:247`, `:268`, `:313`: `collect_groundcover_frame`'s `resident` and `candidates` Vecs and its `emitted` map
- `byroredux/src/render/groundcover.rs:418`: `collect_groundcover_disturbers`' `found` Vec
- Caller: `byroredux/src/app_frame.rs:369` → `collect_and_prepare_groundcover` (`:727-779`). It runs every frame while the ground-cover pipeline exists and `--groundcover-off` is not set.

**Status**: NEW. Introduced by `6f2831d5c` (2026-09-14) and `fd0cd577c` (2026-09-15). At the 09-11 baseline (`b3db49fa`), the file's only hash collection was a test `HashSet`.
**Verified against**: HEAD `73aaed7b9`; the code is identical to the audited `f97775ca8`.

## Description

`GroundCoverResidency::reconcile` allocates all of these every frame:
- `desired: HashMap<ChunkKey, ChunkCandidate>`;
- `wanted: HashSet<ChunkKey>`, which duplicates `desired.contains_key`;
- `resident: HashSet<ChunkKey>`;
- the `pending` Vec and the returned `Vec<(usize, ChunkCandidate)>`.

`collect_groundcover_frame` adds fresh `resident` and `candidates` Vecs and a std `emitted: HashMap<usize, u32>`. `collect_groundcover_disturbers` adds a fresh `found` Vec. The App already keeps persistent scratch for the *outputs* (`groundcover_cells`, `groundcover_chunks`, …) but not for these intermediates.

Every hashed collection here is std SipHash, in `byroredux/src/render/`. `_audit-common.md`'s hot-path rule (#2923) says this path stays `FxHashMap`/`FxHashSet` end to end, and that a reintroduced std map there is the regression.

The comment at `:308-312` has been stale since the residency ring. It reads: "Survivors arrive in walk order, so one cell's chunks are contiguous and only the last emitted cell can match". `reconcile` returns entries in slot order, so `emitted` does need a map, but not a hashed one.

## Evidence

- In production code, `grep -n 'HashMap\|HashSet' byroredux/src/render/groundcover.rs` → `:14`, `:116`, `:120`, `:139`, `:313`. The `:809` hit is inside the `#[cfg(test)]` module.
- `git show b3db49fa:byroredux/src/render/groundcover.rs | grep -n 'HashMap\|HashSet'` finds only a test `HashSet`.
- Draw distance 3000 plus the chunk bound gives about 135 candidates per frame, at about 6 hash operations each. The ring is bounded to a 15×15 window: `radius_chunks = ceil((3000 + 362) / 512) = 7`.

## Impact

*Est.* 20-40 µs of main-thread time on every exterior frame, from about 800-950 SipHash operations plus about 10 heap allocations. The #2923 guard (the `must stay FxHashSet (#2923)` assertion in `crates/renderer/src/vulkan/context/mod.rs`) pins only named `VulkanContext` fields, so it cannot see this site. No quantitative guard exists for it.

## Related

- #2923: the hot-path Fx rule. #3682, #3137 and #3059 are earlier instances of the same class, all closed and fixed at their own sites; this is a new site, not a regression of them.
- #4609 (PERF-D1-2026-09-21-02): the same per-frame ground-cover collector family (atlas and species table rebuilt every frame).
- #4610 (PERF-D8-2026-09-21-02): the App-owned ground-cover scratches have no `ScratchTelemetry` rows.

## Suggested Fix

- Keep persistent `desired`, `pending` and result buffers on `GroundCoverResidency`, reused with clear + extend.
- Use `FxHashMap`, or a grid-indexed Vec over the bounded 15×15 window.
- Drop `wanted` in favour of `desired.contains_key`.
- Make `emitted` a `Vec<Option<u32>>` indexed by the dense `candidate.cell` ordinal, and fix the stale `:308-312` comment.
- Optionally, turn the #2923 guard into a source scan of `byroredux/src/render/` for `std::collections::Hash*`, so the rule is enforced as written.

Source: docs/audits/AUDIT_PERFORMANCE_2026-09-21.md (PERF-D1-2026-09-21-01)

## Completeness Checks
- [ ] **SIBLING**: The other per-frame ground-cover collectors (`collect_groundcover_species`, `collect_groundcover_species_table`, `collect_groundcover_disturbers`) are checked for the same fresh-allocation / std-hash pattern
- [ ] **TESTS**: A regression test pins the fix, for example a source scan of `byroredux/src/render/` production code for `std::collections::Hash*`, or an allocation-count assertion on `reconcile`

