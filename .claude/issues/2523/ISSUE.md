# PERF-D8-NEW-01: allocate_vec's remaining-bytes floor ignores size_of::<T>(), letting corrupt counts amplify into multi-gigabyte pre-read allocations

**GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/2523
**Finding ID**: PERF-D8-NEW-01

**Severity**: HIGH
**Dimension**: NIF Parse Performance
**Location**: `crates/nif/src/stream.rs:258-271` (`allocate_vec`); worst-case call site `crates/nif/src/blocks/node.rs:1081` (`Vec<[f32; 16]>` in `BsDistantObjectInstancedNode::parse`); ~20 additional call sites across `crates/nif/src/blocks/{skin,interpolator,collision/*,tri_shape/*,controller/*}.rs` share the same bound function
**Status**: NEW

## Description
`allocate_vec<T>(count: u32)` bounds `count` against `remaining` — the bytes left in the stream — treating every element as if it costs a minimum of **1 byte**:
```rust
pub fn allocate_vec<T>(&self, count: u32) -> io::Result<Vec<T>> {
    let remaining = total.saturating_sub(pos);
    if (count as usize) > remaining { return Err(...); }
    Ok(Vec::with_capacity(count as usize))   // <-- count elements of size_of::<T>(), not count bytes
}
```
For any `T` whose real size is `size_of::<T>() = k > 1` bytes, a corrupt `count` up to `remaining` (which the check allows) requests a `Vec::with_capacity` of `count * k` bytes — up to **k× the actual data available**. This is exactly the failure mode `MAX_SINGLE_ALLOC_BYTES` / `check_alloc` were built to close for `read_bytes` and `read_pod_vec` (#113, #388, #764) — but `allocate_vec` never calls `check_alloc` and has no `size_of`-aware term at all. Compare with its sibling `read_pod_vec`, which computes `byte_count = count * size_of::<T>()` and passes that through `check_alloc` (both the remaining-bytes check **and** the 256 MB hard cap). `allocate_vec` is missing both.

Worst concrete instance: `BsDistantObjectInstancedNode::parse` (Starfield distant-object-instancing node, live in the block dispatch table) does `stream.allocate_vec::<[f32; 16]>(num_transforms)?` — `size_of::<[f32; 16]>() = 64` bytes. NIF files legitimately range up to the archive-level `MAX_CHUNK_BYTES` cap of 1 GB (vanilla content tops out around 325 MB on FO76's largest BA2). A corrupt/hostile file with e.g. 300 MB remaining and a forged `num_transforms` of 300,000,000 passes the `count > remaining` check (300M ≤ 300M) but requests `Vec::with_capacity::<[f32;16]>(300_000_000)` — **19.2 GB**. Any `allocate_vec::<T>` call with `size_of::<T>() > 1` (most of them — `NiTransform`-sized bone/ragdoll structs, `QuatKey`, `InterpBlendItem`, `BsGeometrySegmentData`, `(u64,u32)` tuples, etc.) is proportionally amplified by its own `size_of::<T>()`.

## Evidence
Confirmed directly at `crates/nif/src/stream.rs:258-271`; call site `crates/nif/src/blocks/node.rs:1057-1081`; sibling function that *does* apply the size-aware cap at `crates/nif/src/stream.rs:355-365`.

## Impact
`Vec::with_capacity` failing an allocation calls Rust's default `handle_alloc_error`, which **aborts the process** — not a recoverable `io::Result::Err`. Even where the host has enough virtual memory, the multi-GB transient reservation is itself a DoS vector (thrash / OS OOM-killer), far outside the crate's own documented "256 MB is well above any legitimate single-block payload" invariant. Because `allocate_vec` is the shared primitive for ~20 non-bulk-array block parsers, the blast radius is every block type that uses it for a non-1-byte-element type, not just `BsDistantObjectInstancedNode`. A prior audit (`AUDIT_INCREMENTAL_2026-07-05.md`, disproof log entry for #1885) examined this bound but only from the false-positive-rejection angle; that reasoning is correct for legitimate files but does not cover the amplification direction addressed here.

## Related
#113, #388, #764, #831 (the `allocate_vec`/`check_alloc` hardening lineage this gap sits inside); #833/#1439 (`read_pod_vec`'s sibling, correctly size-aware); disproof log entry for #1885 in `docs/audits/AUDIT_INCREMENTAL_2026-07-05.md` (addresses a different question, doesn't cover this one).

## Suggested Fix
Give `allocate_vec` the same `check_alloc`-style guard `read_pod_vec` already has, scoped to `T: Sized` with a `checked_mul(count, size_of::<T>())`: reject if the byte product exceeds `remaining` *or* `MAX_SINGLE_ALLOC_BYTES`. This covers essentially all current non-POD-bulk `allocate_vec::<T>` call sites. Leave the existing loose 1-byte-per-element bound only for the small set of heap-indirect element types (`String`, `Vec<T>`, `Option<Arc<str>>`) where on-disk size can legitimately be smaller than `size_of::<T>()` — e.g. via a second helper (`allocate_vec_sized::<T>(count, min_wire_bytes_per_elem)`), so the fix doesn't reintroduce the false-positive risk the 2026-07-05 disproof log correctly flagged for those types.

## Completeness Checks
- [ ] **TESTS**: A regression test constructs a corrupt-count NIF stream that would have amplified past `MAX_SINGLE_ALLOC_BYTES` and confirms it's rejected, not aborted
- [ ] **SIBLING**: All ~20 `allocate_vec::<T>` call sites checked against the new guard; the heap-indirect-element exemption (if added) doesn't reopen the false-positive class #1885's disproof log flagged

