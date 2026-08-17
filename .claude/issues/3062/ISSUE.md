# PERF-D8-01: read_pod_vec's zero-init pre-fill is dead work

**Issue**: #3062
**Severity**: LOW
**Labels**: `low,nif-parser,performance,bug`
**Source report**: `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_PERFORMANCE_2026-08-16.md` (Dimension 8 — parser allocation).

**Location**: `crates/nif/src/stream.rs`:449 (`read_pod_vec`); instantiation list at :62-64

## Description

`read_pod_vec`'s zero-init pre-fill is **dead work** — the buffer is immediately overwritten by the read — and `NiPoint3` is the one instantiation that misses std's `IsZero` fast path.

## Evidence

```rust
// crates/nif/src/stream.rs:449 (re-verified 2026-08-17)
let mut out: Vec<T> = vec![T::default(); count];
```

For types where `T::default()` is all-zero bits, std's `IsZero` specialisation turns this into a `calloc`-style fast path. `NiPoint3` does not qualify, so its instantiation pays a real per-element default-construct pass that the subsequent read discards.

## Impact

Wasted initialisation proportional to vertex/point count on every NIF parse — load-time only, not per-frame, which is why it is LOW. It scales with the largest geometry in the corpus.

## Suggested Fix

Read into an uninitialised or `Vec::with_capacity` + extend-from-slice buffer so the pre-fill disappears for every instantiation, rather than special-casing `NiPoint3`.

**If this uses `unsafe`** (e.g. `set_len` over `MaybeUninit`), the safety comment must state the invariant — that the full length is written before any read — and the read must be infallible up to `count` or the length only set after a successful read. A safe `extend_from_slice` formulation is preferable if it measures the same.

## Related

- `crates/nif/src/stream.rs`'s other `read_*` helpers (check for the same pre-fill)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, the safety comment states the write-before-read invariant and no partial-read path can expose uninitialised memory
- [ ] **MEASURED**: The saving is benchmarked on a real archive parse
- [ ] **ALL-INSTANTIATIONS**: The fix removes the pre-fill generally, not just for `NiPoint3`
- [ ] **SIBLING**: Other `read_*` helpers in `stream.rs` checked for the same pattern
- [ ] **TESTS**: Parse-rate regression tests stay green (14,881 FNV NIFs clean)

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3062 --json state` when live state is needed.*
