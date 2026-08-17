# SCR-D8-01: HkxAnimation::num_frames has no upper bound and scales a Vec::with_capacity by track count

**Issue**: #3011
**Severity**: HIGH
**Dimension**: 8 — Havok packfile reader
**Labels**: `high,safety,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 8 — Havok packfile reader).

**Location**: `crates/hkx/src/animation.rs`:126 (read), :131-145 (the dimension-validation block), :231 (the allocation)

## Description

`num_frames` is read as a raw `u32` from the `.hkx` and reaches a `Vec::with_capacity` **multiplied by the track count**, with no upper bound.

**Premise refined against the live code** (the source report says "unvalidated"): `num_frames` *is* validated — but only against zero. The dimension-validation block bounds `transform_count`, `float_count`, `num_blocks` and `mask_size`, and rejects `num_frames == 0`. It does **not** bound `num_frames` from above.

## Evidence

```rust
// crates/hkx/src/animation.rs:126
let num_frames = pack.u32(object + 0x38, "animation frame count")?;
```

```rust
// :131-145 — every other dimension is bounded; num_frames only against zero
|| transform_count > 4096
|| float_count > 4096
|| num_frames == 0          // <- no upper bound
|| num_blocks > 4096
```

```rust
// :231 — the allocation, scaled by transform_count
let mut tracks = vec![Vec::with_capacity(num_frames as usize); transform_count];
```

Re-verified 2026-08-17: `grep -n "num_frames >" crates/hkx/src/animation.rs` finds no upper-bound check (the only hit is an assertion inside a test).

With `num_frames = 0xFFFFFFFF` and `transform_count` at its permitted ceiling of 4096, this requests ~4.29 × 10⁹ elements per track across 4096 tracks.

## Impact

A crafted or corrupt `.hkx` aborts the process on allocation failure. `crates/hkx` reads **untrusted archive input** (its sole consumer is `byroredux/src/asset_provider/animation.rs`, fed from BSA/BA2), and the crate is on `_audit-common.md`'s un-owned-subsystem list with no parser-discipline dimension of its own.

Abort rather than memory-unsafety — Rust's allocator failure is not UB — so this is a denial-of-service on malformed content, not a security hole.

## Suggested Fix

Add an upper bound on `num_frames` in the existing dimension-validation block, consistent with the 4096 ceilings already applied to its siblings. Better still, bound the **product** `num_frames × transform_count`, since that is the quantity actually allocated.

Prefer `try_reserve` for the allocation so a hostile value degrades to a clean `Err` rather than an abort.

## Related

- SCR-D8-2026-08-16-02 (#3013), SCR-D8-2026-08-16-03 (#3018), SCR-D8-2026-08-16-04 (#3014) — the same crate's other parser-discipline gaps
- `_audit-common.md`'s un-owned-subsystem table (Havok packfile reader)

## Completeness Checks
- [ ] **UNSAFE**: N/A — no `unsafe` involved; the failure is an allocator abort
- [ ] **SIBLING**: Every other `with_capacity` in `crates/hkx` checked against an on-disk-derived count (`:90`, `:169`, `:192`, `:494`, `:539`)
- [ ] **PRODUCT-BOUND**: The bound covers `num_frames × transform_count`, not just each factor
- [ ] **GRACEFUL**: `try_reserve` (or an explicit bound) turns a hostile value into `Err`, not an abort
- [ ] **TESTS**: A negative-input test feeds an out-of-range `num_frames` and asserts `InvalidData`

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3011 --json state` when live state is needed.*
